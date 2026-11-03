// Package terminal owns the reversible terminal session and ANSI cell diff.
package terminal

import (
	"fmt"
	"os"
	"strings"
	"syscall"
	"unsafe"
)

type Session struct {
	saved  syscall.Termios
	closed bool
}
type size struct{ Rows, Cols, X, Y uint16 }

func ioctl(fd uintptr, request uintptr, arg unsafe.Pointer) error {
	_, _, e := syscall.Syscall(syscall.SYS_IOCTL, fd, request, uintptr(arg))
	if e != 0 {
		return e
	}
	return nil
}
func Size() (int, int, error) {
	var s size
	err := ioctl(os.Stdout.Fd(), syscall.TIOCGWINSZ, unsafe.Pointer(&s))
	return int(s.Cols), int(s.Rows), err
}
func Open() (*Session, error) {
	if term := os.Getenv("TERM"); term == "" || term == "dumb" {
		return nil, fmt.Errorf("a VT-compatible terminal is required; use --snapshot for redirected output")
	}
	var t syscall.Termios
	if err := ioctl(os.Stdin.Fd(), getTermios, unsafe.Pointer(&t)); err != nil {
		return nil, fmt.Errorf("stdin is not a terminal; use --snapshot: %w", err)
	}
	if _, _, err := Size(); err != nil {
		return nil, fmt.Errorf("stdout is not a terminal; use --snapshot: %w", err)
	}
	s := &Session{saved: t}
	t.Iflag &^= syscall.BRKINT | syscall.ICRNL | syscall.INPCK | syscall.ISTRIP | syscall.IXON
	t.Lflag &^= syscall.ECHO | syscall.ICANON | syscall.IEXTEN
	// ISIG remains set so Ctrl-C follows the signal cleanup path.
	t.Cflag |= syscall.CS8
	t.Cc[syscall.VMIN] = 1
	t.Cc[syscall.VTIME] = 0
	if err := ioctl(os.Stdin.Fd(), setTermios, unsafe.Pointer(&t)); err != nil {
		return nil, fmt.Errorf("cannot set terminal input mode: %w", err)
	}
	if _, err := fmt.Fprint(os.Stdout, "\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[2J\x1b[H"); err != nil {
		s.Close()
		return nil, err
	}
	return s, nil
}
func (s *Session) Close() {
	if s.closed {
		return
	}
	s.closed = true
	_ = ioctl(os.Stdin.Fd(), setTermios, unsafe.Pointer(&s.saved))
	_, _ = fmt.Fprint(os.Stdout, "\x1b[0m\x1b[?7h\x1b[?25h\x1b[?1049l")
}

type ColorMode int

const (
	Mono ColorMode = iota
	Basic
	Indexed
	TrueColor
)

func DetectColor(option string) ColorMode {
	switch option {
	case "mono":
		return Mono
	case "basic":
		return Basic
	case "256":
		return Indexed
	case "true":
		return TrueColor
	}
	if _, ok := os.LookupEnv("NO_COLOR"); ok {
		return Mono
	}
	if strings.Contains(os.Getenv("COLORTERM"), "truecolor") || strings.Contains(os.Getenv("COLORTERM"), "24bit") {
		return TrueColor
	}
	t := os.Getenv("TERM")
	if strings.Contains(t, "direct") || strings.Contains(t, "truecolor") {
		return TrueColor
	}
	if strings.Contains(t, "256color") {
		return Indexed
	}
	return Basic
}
