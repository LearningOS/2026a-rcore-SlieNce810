# rCore-Tutorial 第 3 章实验报告（lab1）

## 一、功能总结

本章为每个任务维护独立的系统调用计数表，并在系统调用分发前自增，使查询结果包含本次调用。在此基础上实现 `sys_trace` 的三种功能：读取用户地址的一个字节、把 `data` 的低字节写入用户地址、返回当前任务对指定系统调用的调用次数；其余请求返回 `-1`。实现后 `ch3_sleep`、`ch3_sleep1`、`ch3_trace` 均通过，yield 测例不回归。

## 二、简答题

### 1. 三个 bad 测例

在 ch3 内核下运行第 2 章的三个 bad 测例（`make -C os run CHAPTER=2 BASE=1`），现象如下：

| 测例 | 用户态行为 | 运行结果 |
| --- | --- | --- |
| `ch2b_bad_address` | 向地址 `0x0` 写入一个字节 | 触发 `StoreFault`/`StorePageFault`，内核打印 `[kernel] PageFault in application, bad addr = 0x0, bad instruction = 0x... , kernel killed it.`，该任务被终止 |
| `ch2b_bad_instructions` | 执行 S 态特权指令 `sret` | 触发 `IllegalInstruction`，内核打印 `[kernel] IllegalInstruction in application, kernel killed it.`，该任务被终止 |
| `ch2b_bad_register` | 执行 `csrr ..., sstatus` 读取 S 态 CSR | 同样触发 `IllegalInstruction`，被内核终止，`sstatus` 的值不会打印出来 |

三者的共同点是：U 态只能执行非特权指令，也不能访问 S 态 CSR；一旦违反，硬件产生异常并陷入 S 态，内核的 `trap_handler` 捕获后将当前任务标记为 `Exited` 并调度下一个任务，因此其余任务仍能继续运行。

本实验使用的固件为 **RustSBI 0.4.1**（QEMU `virt` 平台、jump 模式；启动日志中的 `RustSBI version 0.4.1` 可直接确认）。

### 2. `trap.S` 分析

#### （1）刚进入 `__restore` 时 `sp` 的含义与两种使用情景

`sp` 指向当前任务内核栈上 `TrapContext` 的起始地址（`__alltraps` 压入的 34×8 字节区域，`x[0]` 所在位置）。

两种情景：

1. `__alltraps` 保存完用户上下文后调用 `trap_handler(cx)`，函数返回后顺序执行到 `__restore`，此时 `sp` 仍指向这次 trap 的 `TrapContext`；
2. 任务第一次被调度：`TaskContext::goto_restore(init_app_cx(i))` 把 `ra` 设为 `__restore`、`sp` 设为预先压入内核栈的初始 `TrapContext`；`__switch` 恢复该 `TaskContext` 后执行 `ret`，直接跳入 `__restore`。

#### （2）L43–L48 特殊处理的寄存器

- `t0` ← `32*8(sp)` 的 `sstatus`，`csrw sstatus` 恢复它：其中的 `SPP` 决定 `sret` 后返回 U 态，`SPIE` 决定返回后是否开中断，因此必须整体恢复；
- `t1` ← `33*8(sp)` 的 `sepc`，`csrw sepc` 设置返回用户态后的 PC；
- `t2` ← `2*8(sp)` 的用户栈指针，`csrw sscratch` 先把它暂存到 `sscratch`，供 L60 交换使用。

这三项一个是 CSR、一个是被通用循环跳过的 `x2`，无法用 `LOAD_GP` 统一恢复，所以单独处理。

#### （3）L50–L56 为何跳过 `x2` 和 `x4`

- `x2` 是 `sp`：用户栈指针不通过通用寄存器恢复，而是先存进 `sscratch`，在 L60 用一条 `csrrw` 同时完成“`sp` 换成用户栈、`sscratch` 换成内核栈顶”。这样在释放 `TrapContext` 之前仍能用内核 `sp` 寻址。
- `x4` 是 `tp`：用户程序不使用 `tp`，而内核需要保留自己的 `tp`（线程指针），因此不保存也不恢复用户 `tp`。

（`x0` 恒为 0，同样不需要恢复。）

#### （4）L60 执行后 `sp` 与 `sscratch` 的值

- `sp` = 该任务的用户栈指针（来自 `TrapContext.x[2]`）；
- `sscratch` = 该任务内核栈顶（`TrapContext` 已释放后的位置）。

#### （5）`__restore` 中发生状态切换的指令

是 `sret`。它把 PC 设为 `sepc`，把特权级设为 `sstatus.SPP`（此处为 U），并把 `SIE` 设为 `SPIE`；由于 `SPP` 保存的是 U，执行 `sret` 后处理器回到用户态。

#### （6）L13 执行后 `sp` 与 `sscratch` 的值

- `sp` = 当前任务的内核栈顶；
- `sscratch` = 用户栈指针。

#### （7）从 U 态进入 S 态由哪条指令发生

系统调用路径上是用户执行 `ecall`：硬件触发 environment call 异常，跳转到 `stvec` 指向的 `__alltraps`，完成 U→S 的切换。时钟中断、非法指令等异常也由硬件在指令边界完成同样的特权级切换。

## 三、测试结果

| 命令 | 结果 |
| --- | --- |
| `make -C os run BASE=1` | 输出 `Test write A OK!`、`Test write B OK!`、`Test write C OK!` |
| `make -C os run BASE=0` | 输出 `Test sleep OK!`、`Test sleep1 passed!`、`string from task trace test`、`Test trace OK!` |
| `make -C ci-user test CHAPTER=3` | `Test passed: 7/7`，并提示 `Report for lab1 found.` |

## 四、Honor Code

我声明：本次实验由我独立完成。除课程提供的框架代码、实验指导书和教材外，我没有抄袭他人的代码或报告；报告中的分析、结论与测试结果均来自我本人对仓库代码的阅读和实际运行。参考过的公开资料已在文中注明。
