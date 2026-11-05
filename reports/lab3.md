# rCore-Tutorial 第 5 章实验报告（lab3）

## 一、功能总结

本章在地址空间之上引入进程：TCB 记录 pid、父子关系、退出码与独立页表，`fork` 复制用户地址空间并返回两次，`exec` 替换当前进程的 ELF，`waitpid` 回收僵尸子进程。实验补充了 `sys_get_time`、`sys_mmap`、`sys_munmap`：通过当前进程 token 与页表翻译安全访问用户内存，校验映射区间重叠与权限。`sys_spawn` 直接以 ELF 创建子进程并加入调度队列，避免了 `fork + exec` 的地址空间复制；`sys_set_priority` 与步长调度器按 `pass` 最小值选择任务，优先级越大 `stride` 越小、获得 CPU 的比例越高。

## 二、简答题

### 1. `fork`、`exec`、`waitpid` 的区别与配合

- `fork` 复制父进程的用户地址空间、TrapContext 与内核栈，返回两次：父进程得到子进程 pid，子进程得到 0；
- `exec` 不创建新进程，只用新 ELF 重建当前进程的 `MemorySet`、重置 `TrapContext` 与 `program_brk`；
- `waitpid` 在父进程中等待指定子进程（`pid == -1` 表示任意子进程），子进程退出后变成 `Zombie` 并保留退出码，父进程取走退出码后子进程才被真正释放。

典型组合是 shell 中 `fork` 后子进程 `exec`，父进程 `waitpid`。

### 2. 为什么子进程退出后要保留为僵尸进程？

子进程的退出码需要被父进程读取。如果退出时立即释放 TCB，父进程之后调用 `waitpid` 就无从获得退出码。因此退出时先把状态设为 `Zombie`、把退出码保存在 TCB 中，等父进程 `waitpid` 取走后再释放。若父进程先退出，则把孤儿子进程挂到 `initproc` 名下，由它负责回收。

### 3. `spawn` 与 `fork + exec` 的差异

`fork + exec` 会先完整复制父进程地址空间，再立刻丢弃并替换成新 ELF，开销大且中间状态没有意义。`spawn` 直接根据 ELF 构造一个新的 `TaskControlBlock`，只复制必要的父子关系，不复制父进程地址空间，因此更适合“创建一个全新程序”的场景。本实验的 `spawn` 还负责把新进程加入父进程的 `children` 与调度队列。

### 4. 步长调度的原理

每个进程维护 `priority`、`stride` 和 `pass`：

- `stride = BIG_STRIDE / priority`，优先级越高，步长越小；
- 调度器每次选择 `pass` 最小的就绪进程；
- 进程被调度后执行 `pass += stride`。

这样在长时间运行中，优先级为 `p` 的进程获得的 CPU 时间近似正比于 `p`。`set_priority` 只接受 `prio >= 2`，设置后返回新优先级；非法值返回 `-1`。

### 5. 进程切换与任务切换的区别

第 3 章的任务共享同一地址空间，切换只需保存 `TaskContext` 并换内核栈。第 5 章的进程拥有独立页表，切换时还必须切换 `satp`（通过 `__restore` 恢复用户 `TrapContext` 中的 token），并且 TCB 需要维护 pid、父子关系、退出码等进程级信息。线程（第 8 章）则共享地址空间但拥有独立栈与寄存器上下文。

### 6. `sys_get_time` 为什么不能直接解引用用户指针？

用户传入的是虚拟地址，内核页表与用户页表不同。需要用当前进程的 token 构造临时页表，把虚拟页翻译成物理页，再按物理地址写入；同时检查 PTE 的 `V/W/U` 位。`TimeVal` 可能跨越两页，因此本实现按页分段拷贝，而不是只翻译首地址。

## 三、测试结果

```text
Test 04_1 OK!
Test 04_4 test OK!
Test 04_5 ummap OK!
Test 04_6 ummap2 OK!
Test set_priority OK!
Test wait OK!
Test waitpid OK!
Test spawn0 OK!
ch5 Usertests passed!
```

`ch5_stride` 的六组 `priority/ratio` 输出正常，说明步长调度生效。

## 四、Honor Code

我声明：本次实验由我独立完成。除课程提供的框架代码、实验指导书和教材外，我没有抄袭他人的代码或报告；报告中的分析、结论与测试结果均来自我本人对仓库代码的阅读和实际运行。参考过的公开资料已在文中注明。
