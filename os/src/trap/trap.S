.option arch, +d
.altmacro
.macro SAVE_GP n
    sd x\n, \n*8(sp)
.endm
.macro LOAD_GP n
    ld x\n, \n*8(sp)
.endm
.macro SAVE_FP n
    fsd f\n, (34+\n)*8(sp)
.endm
.macro LOAD_FP n
    fld f\n, (34+\n)*8(sp)
.endm
    .section .text
    .globl __alltraps
    .globl __restore
    .align 2
__alltraps:
    # sscratch is 0 while running in S-mode, and holds the kernel stack top
    # while running in U-mode.  This lets us tell where the trap came from.
    csrrw sp, sscratch, sp
    bnez sp, .Ltrap_from_user
    # trap from S-mode: sscratch was 0, so sp is 0 and sscratch is the
    # interrupted kernel sp.  Swap back.
    csrrw sp, sscratch, sp
.Ltrap_from_user:
    # now sp->kernel stack
    addi sp, sp, -67*8
    # save general-purpose registers
    sd x1, 1*8(sp)
    # skip sp(x2), we will save it later
    sd x3, 3*8(sp)
    # skip tp(x4), application does not use it
    # save x5~x31
    .set n, 5
    .rept 27
        SAVE_GP %n
        .set n, n+1
    .endr
    # we can use t0/t1/t2 freely, because they were saved on kernel stack
    csrr t0, sstatus
    csrr t1, sepc
    sd t0, 32*8(sp)
    sd t1, 33*8(sp)
    # x2: for a trap from U-mode it is in sscratch; for a trap from S-mode
    # it is the interrupted kernel sp, i.e. sp + FRAME_SIZE.
    csrr t2, sscratch
    bnez t2, .Lhave_sp
    addi t2, sp, 67*8
.Lhave_sp:
    sd t2, 2*8(sp)
    # sscratch = 0 means "currently in S-mode"
    csrw sscratch, zero
    # save f0~f31 and fcsr
    .set n, 0
    .rept 32
        SAVE_FP %n
        .set n, n+1
    .endr
    frcsr t0
    sd t0, 66*8(sp)
    # set input argument of trap_handler(cx: &mut TrapContext)
    mv a0, sp
    call trap_handler

__restore:
    # now sp->kernel stack(after allocated)
    ld t0, 32*8(sp)
    ld t1, 33*8(sp)
    ld t2, 2*8(sp)
    # SPP tells us whether we return to U-mode or S-mode
    andi t3, t0, 0x100
    # keep SIE=0 while restoring; sret will set SIE from SPIE
    andi t0, t0, -3
    csrw sstatus, t0
    csrw sepc, t1
    bnez t3, .Lrestore_to_s

    # ---- return to U-mode ----
    csrw sscratch, t2
    # restore f0~f31 and fcsr
    .set n, 0
    .rept 32
        LOAD_FP %n
        .set n, n+1
    .endr
    ld t0, 66*8(sp)
    fscsr t0
    # restore general-purpose registers except sp/tp
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr
    # release TrapContext on kernel stack
    addi sp, sp, 67*8
    # now sp->kernel stack, sscratch->user stack
    csrrw sp, sscratch, sp
    sret

.Lrestore_to_s:
    # returning to S-mode: sscratch must stay 0
    csrw sscratch, zero
    # restore f0~f31 and fcsr
    .set n, 0
    .rept 32
        LOAD_FP %n
        .set n, n+1
    .endr
    ld t0, 66*8(sp)
    fscsr t0
    # restore general-purpose registers except sp/tp
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
        LOAD_GP %n
        .set n, n+1
    .endr
    # release TrapContext; sp becomes the interrupted kernel sp
    addi sp, sp, 67*8
    sret
