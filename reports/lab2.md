# rCore-Tutorial 第 4 章实验报告（lab2）

## 一、功能总结

本章把内核从“共享物理内存”推进到 SV39 地址空间：每个任务拥有独立的 `MemorySet` 和页表，内核通过 trampoline 在 trap 时切换 `satp`。在此基础上实现了 `mmap`/`munmap`：检查地址与长度对齐、权限位合法、映射区间不与已有区域重叠，再以 `U|R/W/X` 权限建立或释放页映射。同时重写了 `sys_get_time` 与 `sys_trace`：通过当前任务的 token 和页表翻译访问用户内存，检查 PTE 的 `V/R/W/U` 位并支持跨页写入；`sys_trace` 的计数按任务隔离，读、写、查询分别返回字节值、0 与调用次数。

## 二、简答题

### 1. 为什么 `sys_write` / `sys_trace` 不能直接解引用用户指针？

用户传入的是虚拟地址，而内核运行在内核页表中，用户地址在内核页表里通常没有对应映射，直接解引用会访问到错误的内存或触发异常。正确做法是用当前任务的 `satp` token 构造临时 `PageTable`，把虚拟页号翻译成物理页号，再通过物理页读写；同时检查 PTE 的 `V`、`R`、`W`、`U` 位，避免读写未映射、只读或非用户页。

### 2. `mmap` 需要做哪些检查？

- `start` 必须页对齐，`len` 不能为 0；
- `port` 只允许 `R/W/X` 三位，其他位或 0 一律拒绝；
- 映射区间不能与当前地址空间中已有区域重叠；
- 不能映射到 `TRAP_CONTEXT_BASE` 及以上（trap context / trampoline 区域）；
- 成功后把 `port` 左移一位并加上 `U`，转换为 `MapPermission` 建立 `Framed` 区域。

### 3. 为什么 `munmap` 要求 `start` 和 `len` 都页对齐？

页表按页管理，解除映射只能以页为单位。非对齐的 `start` 或 `len` 会覆盖半个页，可能与相邻映射共享同一物理页，导致误删或泄漏；当前实现要求 `[start, start + len)` 与某个已有区域完全一致，再整段 `unmap` 并从 `areas` 中移除。

### 4. 从用户态 trap 到内核时，地址空间如何切换？

trampoline 在用户页表和内核页表中映射到同一个虚拟地址，因此切换 `satp` 后代码仍然连续可执行。`__alltraps` 先把 `TrapContext` 保存到固定虚拟地址 `TRAP_CONTEXT_BASE`，再把 `satp` 切换到内核 token；返回前恢复用户 token，最后 `sret` 回到用户态。`TrapContext` 的物理页号记录在 TCB 中，内核通过它读取用户寄存器与 `sepc`。

### 5. 为什么检查用户地址时必须看 PTE 的 `U` 位？

trampoline、trap context 等内核映射虽然可读，但没有 `U` 位，不应被用户访问。仅检查 `V/R` 会误判：例如 `isize::MAX` 按 SV39 截断后与 trampoline 落在同一个 VPN，若不检查 `U` 位，`trace_read` 就会错误地读到 trampoline 的内容。因此读、写、跨页写都要求 PTE 同时具备 `V`、对应权限和 `U`。

### 6. `sbrk` 与 `mmap` 的区别？

`sbrk` 只调整一段连续的 heap 的 program break，用于扩展或收缩数据段；`mmap` 可以在任意页对齐地址映射任意长度，权限独立，适合大块或非连续的内存需求。两者最终都通过 `MemorySet` 修改页表，但 `sbrk` 复用已有的 `MapArea`，`mmap` 新建独立的 `MapArea`。

## 三、测试结果

```text
Test 04_1 OK!
Test 04_4 test OK!
Test 04_5 ummap OK!
Test 04_6 ummap2 OK!
Test trace_1 OK!
Test sleep1 passed!
Test trace OK!
Test sleep OK!
```

`ch4b_sbrk`、`ch4_mmap1`、`ch4_mmap2` 的失败标记均未出现。

## 四、Honor Code

我声明：本次实验由我独立完成。除课程提供的框架代码、实验指导书和教材外，我没有抄袭他人的代码或报告；报告中的分析、结论与测试结果均来自我本人对仓库代码的阅读和实际运行。参考过的公开资料已在文中注明。
