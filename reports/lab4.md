# rCore-Tutorial 第 6 章实验报告（lab4）

## 一、功能总结

本章在内核中接入 easy-fs 与 virtio 块设备，使用文件描述符表管理打开的文件。实验在 easy-fs 的 `Inode` 上补充了 `stat`、`link`、`unlink`：`stat` 统计根目录中指向同一 inode 的目录项数量作为 `nlink`，并返回 inode 号、类型与大小；`link` 在根目录追加一个指向现有 inode 的目录项；`unlink` 清空对应目录项，当链接数归零时释放数据块与 inode。内核侧实现了 `fstat`、`linkat`、`unlinkat`，并通过页表翻译把 `Stat` 安全写回用户内存。

## 二、简答题

### 1. inode、目录项与硬链接的关系

inode 保存文件的元数据（大小、数据块索引、类型）和数据块；目录项 `DirEntry` 只保存“名字 → inode 号”的映射。硬链接就是多个目录项指向同一个 inode。`nlink` 表示当前有多少个目录项指向该 inode，因此删除一个名字只是去掉一个目录项并把 `nlink` 减一，数据仍然可通过其他名字访问。

### 2. `unlink` 什么时候真正释放文件？

当被删除的目录项是该 inode 的最后一个链接（`nlink == 0`）时，才调用 `clear` 释放全部数据块，并把 inode 号归还给 inode 位图。本实现中 `unlink` 先统计根目录中剩余的有效目录项，若为 0 才执行释放；否则只清空目录项。

### 3. 为什么 `fstat` 不能直接写用户传入的 `Stat` 指针？

用户传入的是虚拟地址，内核页表与用户页表不同。需要先用当前进程的 token 翻译地址，并检查 PTE 的 `W/U` 位；`Stat` 有 64 字节，可能跨页，因此用按页拷贝的 `user_write_bytes` 写回，而不是直接解引用。

### 4. 目录项删除为什么可以只清空而不移动后面的项？

`find`、`ls` 和链接计数都会跳过空名字的目录项，因此清空后逻辑上等价于删除；目录大小保持不变只会浪费少量空间，不影响正确性。若追求空间利用率，可以把最后一个目录项搬到被删除的位置并缩小目录大小。

### 5. 块缓存与一致性

easy-fs 通过 `get_block_cache` 把磁盘块缓存在内存中，读改写都经过同一缓存。`link`/`unlink` 修改目录项后调用 `block_cache_sync_all()` 写回磁盘，保证后续 `find`、`stat` 和重新挂载时能看到一致的结果。

### 6. 为什么 `nlink` 不直接存进 `DiskInode`？

本项目采用的实现通过扫描根目录的目录项实时统计链接数，因此不需要修改 128 字节的 `DiskInode` 磁盘布局，也不需要迁移已有的 fs 镜像。代价是 `stat` 需要遍历目录；在文件数较少时完全可以接受。另一种做法是在 `DiskInode` 中增加 `nlink` 字段并相应减少直接块指针，但需要重新生成镜像。

## 三、测试结果

```text
file_test passed!
Test file0 OK!
Test fstat OK!
Test link OK!
Test mass open/unlink OK!
Test spawn0 OK!
Test set_priority OK!
```

## 四、Honor Code

我声明：本次实验由我独立完成。除课程提供的框架代码、实验指导书和教材外，我没有抄袭他人的代码或报告；报告中的分析、结论与测试结果均来自我本人对仓库代码的阅读和实际运行。参考过的公开资料已在文中注明。
