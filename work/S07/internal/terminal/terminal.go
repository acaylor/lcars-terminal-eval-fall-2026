//go:build linux || darwin

// Package terminal owns the terminal lifecycle, including failure cleanup.
package terminal

import (
	"fmt"
	"io"
	"os"
	"sync"
	"syscall"
	"unsafe"
)

const EnterSequence = "\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[0m"
const LeaveSequence = "\x1b[0m\x1b[?7h\x1b[?25h\x1b[?1049l"

type Session struct {
	input, output *os.File
	original      syscall.Termios
	once          sync.Once
	closeErr      error
}

func ioctl(fd, request uintptr, value unsafe.Pointer) error {
	_, _, err := syscall.Syscall(syscall.SYS_IOCTL, fd, request, uintptr(value))
	if err != 0 {
		return err
	}
	return nil
}
func Size(file *os.File) (int, int, error) {
	var size struct{ Rows, Cols, X, Y uint16 }
	if err := ioctl(file.Fd(), syscall.TIOCGWINSZ, unsafe.Pointer(&size)); err != nil {
		return 0, 0, err
	}
	if size.Cols == 0 || size.Rows == 0 {
		return 0, 0, fmt.Errorf("terminal reports zero dimensions")
	}
	return int(size.Cols), int(size.Rows), nil
}
func Open(input, output *os.File) (*Session, error) {
	s := &Session{input: input, output: output}
	if err := ioctl(input.Fd(), getTermios, unsafe.Pointer(&s.original)); err != nil {
		return nil, fmt.Errorf("interactive input requires a terminal; use --snapshot for redirected output: %w", err)
	}
	if _, _, err := Size(output); err != nil {
		return nil, fmt.Errorf("interactive output requires a terminal; use --snapshot: %w", err)
	}
	raw := s.original
	raw.Iflag &^= syscall.BRKINT | syscall.ICRNL | syscall.INPCK | syscall.ISTRIP | syscall.IXON
	raw.Oflag &^= syscall.OPOST
	raw.Cflag |= syscall.CS8
	// Keep ISIG: Ctrl-C still raises SIGINT. Disable job-control characters so a
	// suspended process cannot strand a terminal in raw mode.
	raw.Lflag &^= syscall.ECHO | syscall.ICANON | syscall.IEXTEN
	raw.Cc[syscall.VMIN] = 1
	raw.Cc[syscall.VTIME] = 0
	raw.Cc[syscall.VSUSP] = 0
	if err := ioctl(input.Fd(), setTermios, unsafe.Pointer(&raw)); err != nil {
		return nil, fmt.Errorf("cannot set terminal input mode: %w", err)
	}
	if _, err := io.WriteString(output, EnterSequence); err != nil {
		_ = s.Close()
		return nil, fmt.Errorf("cannot initialize terminal: %w", err)
	}
	return s, nil
}
func (s *Session) Close() error {
	s.once.Do(func() {
		_, outErr := io.WriteString(s.output, LeaveSequence)
		s.closeErr = ioctl(s.input.Fd(), setTermios, unsafe.Pointer(&s.original))
		if s.closeErr == nil {
			s.closeErr = outErr
		}
	})
	return s.closeErr
}
