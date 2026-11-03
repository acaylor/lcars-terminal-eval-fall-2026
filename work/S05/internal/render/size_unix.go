// Terminal size via syscall ioctl — replaces the x/term GetSize dependency.
package render

import (
	"syscall"
	"unsafe"
)

type winsize struct {
	rows, cols uint16
	xpix, ypix uint16
}

func ioctlWinsize(fd uintptr) (int, int, bool) {
	var ws winsize
	_, _, errno := syscall.Syscall(syscall.SYS_IOCTL, fd,
		syscall.TIOCGWINSZ, uintptr(unsafe.Pointer(&ws)))
	if errno != 0 || ws.cols == 0 {
		return 0, 0, false
	}
	return int(ws.cols), int(ws.rows), true
}

func readSize() (int, int, bool) {
	if w, h, ok := ioctlWinsize(uintptr(syscall.Stdout)); ok {
		return w, h, true
	}
	if w, h, ok := ioctlWinsize(uintptr(syscall.Stdin)); ok {
		return w, h, true
	}
	return 80, 24, false
}
