import Darwin

package func processHasExited(_ pid: pid_t) -> Bool {
    pid > 0 && kill(pid, 0) == -1 && errno == ESRCH
}
