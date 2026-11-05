# rCore-Tutorial 第 8 章实验报告（lab5）

## 一、功能总结

本章在进程内部引入线程：`ProcessControlBlock` 管理地址空间、文件描述符、信号与同步对象，`TaskControlBlock` 表示线程并拥有独立的用户栈、TrapContext 与内核栈；`thread_create`/`waittid` 基于共享地址空间创建和回收线程。同步方面实现了互斥锁、信号量与条件变量，并在此基础上加入死锁检测：开启检测后，记录线程持有的信号量与正在等待的信号量，在可能阻塞时沿等待图递归检查；只有请求的信号量的所有持有者都处于阻塞状态且形成环时才返回 `-0xdead`。互斥锁记录持有线程，同一线程重复加锁会直接报告死锁。

## 二、简答题

### 1. 进程与线程的区别

进程是资源分配的单位，拥有独立的地址空间、文件描述符表、子进程关系和信号处理状态；线程是调度的单位，同一进程内的多个线程共享地址空间和文件描述符，但各自拥有用户栈、内核栈、TrapContext 与寄存器上下文。进程切换需要切换页表，线程切换在同一地址空间内只需切换栈与上下文。

### 2. 线程创建与回收

`thread_create(entry, arg)` 在当前进程内分配 tid、用户栈和 TrapContext，构造新的 `TaskControlBlock` 并加入调度队列，返回 tid。线程结束时不能直接释放资源：需要把线程标记为 `Zombie` 并保留退出码，等 `waittid` 回收；最后一个线程退出时整个进程才进入僵尸状态。

### 3. 互斥锁、信号量、条件变量的区别

- 互斥锁强调所有权，同一时刻只有一个线程持有，适合临界区；
- 信号量是计数器，可以被任意线程 up/down，适合资源计数与事件通知；
- 条件变量必须与互斥锁配合：等待时原子地释放锁并阻塞，被唤醒后重新获取锁，适合“等待某个条件成立”。

### 4. 死锁检测的等待图

把“线程等待某个信号量”看作等待边：若线程 T 请求信号量 S 且 S 的计数为 0，则 T 需要等待 S 的当前持有者释放。记录每个线程持有的信号量（成功 down 且尚未 up）和正在等待的信号量后，可以从 T 出发递归检查：

- 若 T 请求的 S 没有任何持有者，无法判断，按不阻塞处理；
- 只有 S 的**所有**持有者都处于阻塞状态，并且依赖链最终回到 T，才判定为死锁；
- 只要存在一个可运行的持有者，它迟早会 up，T 就不算死锁。

这样第 2 个信号量测试（有额外持有者可释放）不会被误报，而第 1 个测试中的环（T1 等 sem1、T2 等 sem3、T3 等 sem2）会被检出。

### 5. 为什么互斥锁可以单独判断同一线程重复加锁？

`MutexBlocking` 记录 `owner`（持有线程的 tid）。同一线程再次加锁时，锁不会被自己释放，必然永久阻塞；因此在开启检测时直接返回 `-0xdead`，不需要构造完整等待图。解锁时若唤醒等待者，则把 owner 转交给被唤醒线程。

### 6. 阻塞与自旋的取舍

自旋锁不让出 CPU，适合临界区极短、且不能睡眠的场景（如内核中的 `UPSafeCell`）；阻塞锁在拿不到锁时调用 `block_current_and_run_next` 让出 CPU，适合可能长时间持有的用户态锁。本章用户态互斥锁采用阻塞实现，避免忙等浪费整个时间片。

## 三、测试结果

```text
Test file0 OK!
Test fstat OK!
Test link OK!
Test mass open/unlink OK!
file_test passed!
pipetest passed!
pipe_large_test passed!
mpsc_sem passed!
philosopher dining problem with mutex test passed!
race adder using spin mutex test passed!
sync_sem passed!
test_condvar passed!
threads with arg test passed!
threads test passed!
deadlock test mutex 1 OK!
deadlock test semaphore 1 OK!
deadlock test semaphore 2 OK!
```

## 四、Honor Code

我声明：本次实验由我独立完成。除课程提供的框架代码、实验指导书和教材外，我没有抄袭他人的代码或报告；报告中的分析、结论与测试结果均来自我本人对仓库代码的阅读和实际运行。参考过的公开资料已在文中注明。
