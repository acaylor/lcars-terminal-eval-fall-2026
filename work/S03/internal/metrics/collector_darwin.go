//go:build darwin && cgo

package metrics

/*
#cgo LDFLAGS: -lproc
#include <mach/mach.h>
#include <mach/host_info.h>
#include <mach/processor_info.h>
#include <sys/sysctl.h>
#include <sys/time.h>
#include <sys/mount.h>
#include <sys/resource.h>
#include <sys/proc_info.h>
#include <libproc.h>
#include <ifaddrs.h>
#include <net/if.h>
#include <net/if_dl.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

typedef struct { uint64_t busy,total; } cpu_counter;
typedef struct { char name[IFNAMSIZ]; uint64_t rx,tx; } net_counter;
typedef struct { int pid; char name[256]; uint64_t user,system,memory,born; } proc_counter;
typedef struct {
 unsigned flags;
 double uptime,load[3]; uint64_t memtotal,memavailable,swaptotal,swapused,disktotal,diskused;
 cpu_counter *cpus; unsigned cores;
 net_counter *nets; unsigned netcount;
 proc_counter *procs; unsigned proccount,visiblecount;
} native_sample;

static native_sample native_collect(int processes) {
 native_sample r={0}; size_t len; struct timeval boot,now;
 len=sizeof(boot);
 if(sysctlbyname("kern.boottime",&boot,&len,NULL,0)==0 && gettimeofday(&now,NULL)==0) {
  r.uptime=(double)(now.tv_sec-boot.tv_sec)+(now.tv_usec-boot.tv_usec)/1e6; r.flags|=1;
 }
 if(getloadavg(r.load,3)==3) r.flags|=2;
 len=sizeof(r.memtotal);
 if(sysctlbyname("hw.memsize",&r.memtotal,&len,NULL,0)==0) r.flags|=4;
 mach_port_t host=mach_host_self(); vm_size_t pagesize=0;
 vm_statistics64_data_t vm; mach_msg_type_number_t count=HOST_VM_INFO64_COUNT;
 if(host_page_size(host,&pagesize)==KERN_SUCCESS && host_statistics64(host,HOST_VM_INFO64,(host_info64_t)&vm,&count)==KERN_SUCCESS) {
  // XNU already includes speculative pages in free_count. Inactive is reclaimable.
  r.memavailable=((uint64_t)vm.free_count+vm.inactive_count)*pagesize;r.flags|=8;
 }
 processor_info_array_t info=NULL; natural_t cores=0; mach_msg_type_number_t ninfo=0;
 if(host_processor_info(host,PROCESSOR_CPU_LOAD_INFO,&cores,&info,&ninfo)==KERN_SUCCESS) {
  r.cpus=calloc((size_t)cores+1,sizeof(cpu_counter));
  if(r.cpus) {r.cores=cores;for(unsigned i=0;i<cores;i++) {
   uint64_t total=0;for(int j=0;j<CPU_STATE_MAX;j++) total+=(uint32_t)info[i*CPU_STATE_MAX+j];
   uint64_t busy=total-(uint32_t)info[i*CPU_STATE_MAX+CPU_STATE_IDLE];
   r.cpus[i+1]=(cpu_counter){busy,total};r.cpus[0].busy+=busy;r.cpus[0].total+=total;
  }}
  vm_deallocate(mach_task_self(),(vm_address_t)info,(vm_size_t)ninfo*sizeof(integer_t));
 }
 mach_port_deallocate(mach_task_self(),host);
 struct xsw_usage swap;len=sizeof(swap);
 if(sysctlbyname("vm.swapusage",&swap,&len,NULL,0)==0) {r.swaptotal=swap.xsu_total;r.swapused=swap.xsu_used;r.flags|=16;}
 struct statfs fs;
 if(statfs("/",&fs)==0) {r.disktotal=(uint64_t)fs.f_blocks*fs.f_bsize;r.diskused=(uint64_t)(fs.f_blocks-fs.f_bfree)*fs.f_bsize;r.flags|=32;}
 struct ifaddrs *ifs=NULL;
 if(getifaddrs(&ifs)==0) {
  unsigned n=0;for(struct ifaddrs *p=ifs;p;p=p->ifa_next) if(p->ifa_addr && p->ifa_addr->sa_family==AF_LINK && !(p->ifa_flags&IFF_LOOPBACK) && p->ifa_data)n++;
  r.nets=calloc(n?n:1,sizeof(net_counter));
  if(r.nets){r.flags|=64;for(struct ifaddrs *p=ifs;p;p=p->ifa_next) {
   if(!p->ifa_addr || p->ifa_addr->sa_family!=AF_LINK || (p->ifa_flags&IFF_LOOPBACK) || !p->ifa_data)continue;
   struct if_data *data=(struct if_data*)p->ifa_data;net_counter *net=&r.nets[r.netcount++];
   strlcpy(net->name,p->ifa_name,sizeof(net->name));net->rx=data->ifi_ibytes;net->tx=data->ifi_obytes;
  }}freeifaddrs(ifs);
 }
 if(processes) {
  int capacity=proc_listallpids(NULL,0);
  if(capacity>0) {
   capacity+=256;int *pids=calloc((size_t)capacity,sizeof(int));
   if(pids){int n=proc_listallpids(pids,capacity*(int)sizeof(int));
    if(n>=0){r.visiblecount=n;r.flags|=128;r.procs=calloc(n?n:1,sizeof(proc_counter));
     if(r.procs)for(int i=0;i<n;i++) {
      struct rusage_info_v2 usage;
      if(pids[i]<=0 || proc_pid_rusage(pids[i],RUSAGE_INFO_V2,(rusage_info_t*)&usage)!=0)continue;
      proc_counter *p=&r.procs[r.proccount++];p->pid=pids[i];p->user=usage.ri_user_time;p->system=usage.ri_system_time;
      p->memory=usage.ri_resident_size;p->born=usage.ri_proc_start_abstime;
      if(proc_name(pids[i],p->name,sizeof(p->name))<=0)strlcpy(p->name,"unavailable",sizeof(p->name));
     }
    }free(pids);
   }
  }
 }
 return r;
}
static void native_free(native_sample *r) {free(r->cpus);free(r->nets);free(r->procs);}
*/
import "C"

import (
	"os"
	"runtime"
	"time"
	"unsafe"
)

type nativeCollector struct{ host string }

func NewCollector() Collector { host, _ := os.Hostname(); return &nativeCollector{host: host} }
func (c *nativeCollector) Collect(processes bool) Raw {
	r := Raw{At: time.Now(), Host: c.host, OS: "macOS", Arch: runtime.GOARCH}
	p := 0
	if processes {
		p = 1
	}
	n := C.native_collect(C.int(p))
	defer C.native_free(&n)
	flag := func(mask uint) bool { return uint(n.flags)&mask != 0 }
	if flag(1) {
		r.Uptime = Known(float64(n.uptime))
	} else {
		r.Issues = append(r.Issues, "uptime")
	}
	if flag(2) {
		r.Load1 = Known(float64(n.load[0]))
		r.Load5 = Known(float64(n.load[1]))
		r.Load15 = Known(float64(n.load[2]))
	} else {
		r.Issues = append(r.Issues, "load")
	}
	if flag(4) {
		r.MemTotal = Known(float64(n.memtotal))
	}
	if flag(8) {
		r.MemAvailable = Known(float64(n.memavailable))
	}
	if !flag(4) || !flag(8) {
		r.Issues = append(r.Issues, "memory")
	}
	if flag(16) {
		r.SwapTotal = Known(float64(n.swaptotal))
		r.SwapUsed = Known(float64(n.swapused))
	} else {
		r.Issues = append(r.Issues, "swap")
	}
	if flag(32) {
		r.DiskTotal = Known(float64(n.disktotal))
		r.DiskUsed = Known(float64(n.diskused))
	} else {
		r.Issues = append(r.Issues, "filesystem")
	}
	if n.cpus != nil {
		for _, v := range unsafe.Slice(n.cpus, int(n.cores)+1) {
			r.CPUs = append(r.CPUs, CPU{uint64(v.busy), uint64(v.total)})
		}
	} else {
		r.Issues = append(r.Issues, "cpu")
	}
	if flag(64) {
		r.Network = map[string]Network{}
		for _, v := range unsafe.Slice(n.nets, int(n.netcount)) {
			r.Network[C.GoString(&v.name[0])] = Network{uint64(v.rx), uint64(v.tx)}
		}
	} else {
		r.Issues = append(r.Issues, "network")
	}
	if flag(128) {
		r.ProcessCount = Known(float64(n.visiblecount))
		for _, v := range unsafe.Slice(n.procs, int(n.proccount)) {
			r.Processes = append(r.Processes, Process{PID: int(v.pid), Name: C.GoString(&v.name[0]), CPUSeconds: (float64(v.user) + float64(v.system)) / 1e9, Memory: uint64(v.memory), Identity: uint64(v.born), MemoryOK: true})
		}
	} else if processes {
		r.Issues = append(r.Issues, "processes")
	}
	return r
}
