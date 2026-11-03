//go:build linux

package terminal

import "syscall"

const getTermios = syscall.TCGETS
const setTermios = syscall.TCSETS
