//go:build darwin

package terminal

import "syscall"

const getTermios = syscall.TIOCGETA
const setTermios = syscall.TIOCSETA
