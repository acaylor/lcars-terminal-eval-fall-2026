//go:build darwin && cgo

package metrics

/*
#cgo LDFLAGS: -lproc
#include <stdlib.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <sys/sysctl.h>
#include <sys/time.h>
#include <sys/mount.h>
#include <sys/socket.h>
#include <mach/mach.h>
#include <mach/mach_host.h>
#include <mach/processor_info.h>
#include <mach/vm_statistics.h>
#include <libproc.h>
#include <net/if.h>
#include <net/route.h>

typedef struct { uint64_t total, idle; } lc_cpu;
typedef struct { char name[IFNAMSIZ+24]; uint64_t rx, tx; } lc_net;
typedef struct { int pid; char name[64]; uint64_t start, memory; double seconds; } lc_proc;
typedef struct {
  uint64_t mem_total, mem_available, swap_total, swap_used, disk_total, disk_used, disk_available;
  int memory_ok, swap_ok, disk_ok, uptime_ok, load_ok;
  double uptime, load[3];
} lc_system;

static void lc_system_read(lc_system *s) {
  size_t size = sizeof(s->mem_total);
  int total_ok = sysctlbyname("hw.memsize", &s->mem_total, &size, NULL, 0) == 0;
  mach_port_t host = mach_host_self();
  vm_statistics64_data_t vm;
  mach_msg_type_number_t count = HOST_VM_INFO64_COUNT;
  vm_size_t page = 0;
  if (total_ok && host_page_size(host, &page) == KERN_SUCCESS &&
      host_statistics64(host, HOST_VM_INFO64, (host_info64_t)&vm, &count) == KERN_SUCCESS) {
    // Reclaimable estimate: inactive + free. Speculative pages are already in free_count.
    s->mem_available = ((uint64_t)vm.free_count + vm.inactive_count) * page;
    if (s->mem_available <= s->mem_total) s->memory_ok = 1;
  }
  mach_port_deallocate(mach_task_self(), host);
  struct xsw_usage swap;
  size = sizeof(swap);
  if (sysctlbyname("vm.swapusage", &swap, &size, NULL, 0) == 0) {
    s->swap_total = swap.xsu_total; s->swap_used = swap.xsu_used; s->swap_ok = 1;
  }
  struct timeval boot, now;
  size = sizeof(boot);
  if (sysctlbyname("kern.boottime", &boot, &size, NULL, 0) == 0 && gettimeofday(&now, NULL) == 0) {
    s->uptime = (double)(now.tv_sec - boot.tv_sec) + (now.tv_usec - boot.tv_usec) / 1e6;
    s->uptime_ok = s->uptime >= 0;
  }
  s->load_ok = getloadavg(s->load, 3) == 3;
  struct statfs disk;
  if (statfs("/", &disk) == 0 && disk.f_blocks > 0 && disk.f_bfree <= disk.f_blocks) {
    s->disk_total = disk.f_blocks * disk.f_bsize;
    s->disk_used = (disk.f_blocks - disk.f_bfree) * disk.f_bsize;
    s->disk_available = disk.f_bavail * disk.f_bsize; s->disk_ok = 1;
  }
}

static lc_cpu *lc_cpus(int *length) {
  processor_info_array_t info = NULL;
  mach_msg_type_number_t count = 0;
  natural_t n = 0;
  mach_port_t host = mach_host_self();
  kern_return_t result = host_processor_info(host, PROCESSOR_CPU_LOAD_INFO, &n, &info, &count);
  mach_port_deallocate(mach_task_self(), host);
  if (result != KERN_SUCCESS) return NULL;
  lc_cpu *cpus = calloc(n, sizeof(lc_cpu));
  if (cpus) {
    for (natural_t i = 0; i < n; i++) {
      for (int j = 0; j < CPU_STATE_MAX; j++) cpus[i].total += (uint32_t)info[i*CPU_STATE_MAX+j];
      cpus[i].idle = (uint32_t)info[i*CPU_STATE_MAX+CPU_STATE_IDLE];
    }
    *length = (int)n;
  }
  vm_deallocate(mach_task_self(), (vm_address_t)info, count * sizeof(integer_t));
  return cpus;
}

static lc_net *lc_network(int *length) {
  // NET_RT_IFLIST2 supplies 64-bit counters; getifaddrs exposes legacy if_data.
  int mib[6] = {CTL_NET, PF_ROUTE, 0, 0, NET_RT_IFLIST2, 0};
  size_t size = 0;
  if (sysctl(mib, 6, NULL, &size, NULL, 0) != 0 || size > 16*1024*1024) return NULL;
  size += 4096;
  char *data = malloc(size);
  if (!data) return NULL;
  if (sysctl(mib, 6, data, &size, NULL, 0) != 0) { free(data); return NULL; }
  lc_net *nets = calloc(size / sizeof(struct if_msghdr2) + 1, sizeof(lc_net));
  if (!nets) { free(data); return NULL; }
  for (size_t offset = 0; offset + 4 <= size;) {
    struct if_msghdr2 *msg = (struct if_msghdr2 *)(data + offset);
    if (msg->ifm_msglen < 4 || msg->ifm_msglen > size - offset) break;
    if (msg->ifm_type == RTM_IFINFO2 && msg->ifm_msglen >= sizeof(*msg) && !(msg->ifm_flags & IFF_LOOPBACK)) {
      lc_net *net = &nets[(*length)++];
      snprintf(net->name, sizeof(net->name), "if/%u", msg->ifm_index);
      net->rx = msg->ifm_data.ifi_ibytes; net->tx = msg->ifm_data.ifi_obytes;
    }
    offset += msg->ifm_msglen;
  }
  free(data);
  return nets;
}

static lc_proc *lc_processes(int *length, int *total, int *ok) {
  int bytes = proc_listpids(PROC_ALL_PIDS, 0, NULL, 0);
  if (bytes <= 0 || bytes > 64*1024*1024) return NULL;
  // Slack handles normal process churn between sizing and enumeration.
  bytes += 4096;
  int *pids = malloc(bytes);
  if (!pids) return NULL;
  int used = proc_listpids(PROC_ALL_PIDS, 0, pids, bytes);
  if (used <= 0) { free(pids); return NULL; }
  lc_proc *procs = calloc(used / sizeof(int), sizeof(lc_proc));
  if (!procs) { free(pids); return NULL; }
  *ok = 1;
  for (int i = 0; i < used / (int)sizeof(int); i++) {
    if (pids[i] <= 0) continue;
    (*total)++;
    struct proc_taskallinfo task;
    if (proc_pidinfo(pids[i], PROC_PIDTASKALLINFO, 0, &task, sizeof(task)) != sizeof(task)) continue;
    lc_proc *p = &procs[(*length)++];
    p->pid = pids[i];
    p->start = task.pbsd.pbi_start_tvsec * 1000000 + task.pbsd.pbi_start_tvusec;
    p->memory = task.ptinfo.pti_resident_size;
    p->seconds = ((double)task.ptinfo.pti_total_user + task.ptinfo.pti_total_system) / 1e9;
    const char *name = task.pbsd.pbi_name[0] ? task.pbsd.pbi_name : task.pbsd.pbi_comm;
    snprintf(p->name, sizeof(p->name), "%.*s", (int)(task.pbsd.pbi_name[0] ? sizeof(task.pbsd.pbi_name) : sizeof(task.pbsd.pbi_comm)), name);
  }
  free(pids);
  return procs;
}
*/
import "C"

import (
	"fmt"
	"os"
	"runtime"
	"strings"
	"syscall"
	"time"
	"unsafe"
)

type nativeCollector struct{ host, osName string }

func NewCollector() Collector {
	host, _ := os.Hostname()
	version, _ := syscall.Sysctl("kern.osproductversion")
	return &nativeCollector{host: host, osName: strings.TrimSpace("macOS " + version)}
}

func (c *nativeCollector) Collect(details bool) Raw {
	r := Raw{At: time.Now(), Host: c.host, OS: c.osName, Arch: runtime.GOARCH, CPU: make(map[string]CPUCounter)}
	var system C.lc_system
	C.lc_system_read(&system)
	r.Uptime = time.Duration(float64(system.uptime) * float64(time.Second))
	r.UptimeOK = system.uptime_ok != 0
	r.LoadOK = system.load_ok != 0
	for i := range r.Load {
		r.Load[i] = float64(system.load[i])
	}
	if system.memory_ok != 0 {
		r.Memory = Capacity{uint64(system.mem_total), uint64(system.mem_total - system.mem_available), uint64(system.mem_available), true}
	}
	if system.swap_ok != 0 && system.swap_used <= system.swap_total {
		r.Swap = Capacity{uint64(system.swap_total), uint64(system.swap_used), uint64(system.swap_total - system.swap_used), true}
	}
	if system.disk_ok != 0 {
		r.Disk = Capacity{uint64(system.disk_total), uint64(system.disk_used), uint64(system.disk_available), true}
	}
	var count C.int
	cpus := C.lc_cpus(&count)
	if cpus != nil {
		var all CPUCounter
		for i, cpu := range unsafe.Slice(cpus, int(count)) {
			v := CPUCounter{uint64(cpu.total), uint64(cpu.idle)}
			r.CPU[fmt.Sprintf("cpu%d", i)] = v
			all.Total += v.Total
			all.Idle += v.Idle
		}
		r.CPU["cpu"] = all
		C.free(unsafe.Pointer(cpus))
	} else {
		r.Issues = append(r.Issues, "CPU unavailable")
	}
	count = 0
	nets := C.lc_network(&count)
	if nets != nil {
		r.Network = make(map[string]NetCounter)
		for _, net := range unsafe.Slice(nets, int(count)) {
			r.Network[C.GoString(&net.name[0])] = NetCounter{uint64(net.rx), uint64(net.tx)}
		}
		C.free(unsafe.Pointer(nets))
	} else {
		r.Issues = append(r.Issues, "network unavailable")
	}
	if details {
		count = 0
		var total, ok C.int
		procs := C.lc_processes(&count, &total, &ok)
		r.ProcessesOK = ok != 0
		r.ProcessCount = int(total)
		r.Processes = make(map[int]Process)
		if procs != nil {
			for _, p := range unsafe.Slice(procs, int(count)) {
				r.Processes[int(p.pid)] = Process{PID: int(p.pid), Name: C.GoString(&p.name[0]), Start: uint64(p.start), Seconds: float64(p.seconds), Memory: uint64(p.memory)}
			}
			C.free(unsafe.Pointer(procs))
		}
	}
	if !r.Memory.OK {
		r.Issues = append(r.Issues, "memory unavailable")
	}
	if !r.Disk.OK {
		r.Issues = append(r.Issues, "root filesystem unavailable")
	}
	return r
}
