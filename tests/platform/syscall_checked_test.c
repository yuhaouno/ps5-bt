/* SPDX-License-Identifier: GPL-3.0-or-later */
#include <assert.h>
long __wrap___crt_syscall(long, ...);
int main(void)
{
    assert(__wrap___crt_syscall(0, 11L, 22L, 33L, 44L, 55L, 66L) == 66);
    assert(__wrap___crt_syscall(1, 11L, 22L, 33L, 44L, 55L, 66L) == -13);
    assert(__wrap___crt_syscall(2, 11L, 22L, 33L, 44L, 55L, 66L) == -1);
    assert(__wrap___crt_syscall(477, 11L, 22L, 33L, 44L, 55L, 66L) == -1);
    return 0;
}
