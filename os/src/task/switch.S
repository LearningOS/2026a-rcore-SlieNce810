.altmacro
.macro SAVE_SN n
    sd s\n, (\n+2)*8(a0)
.endm
.macro LOAD_SN n
    ld s\n, (\n+2)*8(a1)
.endm
    .section .text
    .globl __switch
__switch:
    # t0 = time at entry (ticks)
    rdtime t0
    # __switch(
    #     current_task_cx_ptr: *mut TaskContext,
    #     next_task_cx_ptr: *const TaskContext
    # )
    # save kernel stack of current task
    sd sp, 8(a0)
    # save ra & s0~s11 of current execution
    sd ra, 0(a0)
    .set n, 0
    .rept 12
        SAVE_SN %n
        .set n, n + 1
    .endr
    # restore ra & s0~s11 of next execution
    ld ra, 0(a1)
    .set n, 0
    .rept 12
        LOAD_SN %n
        .set n, n + 1
    .endr
    # restore kernel stack of next task
    ld sp, 8(a1)
    # SWITCH_TICKS += now - t0; SWITCH_COUNT += 1
    rdtime t1
    sub t1, t1, t0
    la t2, SWITCH_TICKS
    ld t3, 0(t2)
    add t3, t3, t1
    sd t3, 0(t2)
    la t2, SWITCH_COUNT
    ld t3, 0(t2)
    addi t3, t3, 1
    sd t3, 0(t2)
    ret
