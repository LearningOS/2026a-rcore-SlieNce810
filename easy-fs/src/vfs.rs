use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};

/// Metadata of an inode
pub struct InodeStat {
    /// inode number
    pub ino: u32,
    /// file type and mode
    pub mode: u32,
    /// number of hard links
    pub nlink: u32,
    /// size in bytes
    pub size: u32,
}

/// Virtual filesystem layer over easy-fs
pub struct Inode {
    block_id: usize,
    block_offset: usize,
    inode_id: u32,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        block_id: u32,
        block_offset: usize,
        inode_id: u32,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            inode_id,
            fs,
            block_device,
        }
    }

    /// Number of directory entries referring to this inode.
    fn count_links(&self, root: &DiskInode) -> u32 {
        let file_count = (root.size as usize) / DIRENT_SZ;
        let mut nlink = 0;
        for i in 0..file_count {
            let mut dirent = DirEntry::empty();
            if root.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device) == DIRENT_SZ
                && !dirent.name().is_empty()
                && dirent.inode_id() == self.inode_id
            {
                nlink += 1;
            }
        }
        nlink
    }

    /// Get the metadata of this inode
    pub fn stat(&self) -> InodeStat {
        let (is_dir, size) = self.read_disk_inode(|disk_inode| (disk_inode.is_dir(), disk_inode.size));
        let fs = self.fs.lock();
        let (root_block, root_offset) = fs.get_disk_inode_pos(0);
        let nlink = get_block_cache(root_block as usize, Arc::clone(&self.block_device))
            .lock()
            .read(root_offset, |root: &DiskInode| self.count_links(root));
        InodeStat {
            ino: self.inode_id,
            mode: if is_dir { 0o040000 } else { 0o100000 },
            nlink,
            size,
        }
    }

    /// Add a hard link named `name` in the root directory to this inode
    pub fn link(&self, name: &str) -> bool {
        let mut fs = self.fs.lock();
        let (root_block, root_offset) = fs.get_disk_inode_pos(0);
        let success = get_block_cache(root_block as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(root_offset, |root: &mut DiskInode| {
                if self.find_inode_id(name, root).is_some() {
                    return false;
                }
                let file_count = (root.size as usize) / DIRENT_SZ;
                let new_size = ((file_count + 1) * DIRENT_SZ) as u32;
                let blocks_needed = root.blocks_num_needed(new_size);
                let mut new_blocks = Vec::new();
                for _ in 0..blocks_needed {
                    new_blocks.push(fs.alloc_data());
                }
                root.increase_size(new_size, new_blocks, &self.block_device);
                let dirent = DirEntry::new(name, self.inode_id);
                root.write_at(file_count * DIRENT_SZ, dirent.as_bytes(), &self.block_device);
                true
            });
        block_cache_sync_all();
        success
    }

    /// Remove the hard link `name` from the root directory
    pub fn unlink(&self, name: &str) -> bool {
        let (success, remaining) = {
            let fs = self.fs.lock();
            let (root_block, root_offset) = fs.get_disk_inode_pos(0);
            get_block_cache(root_block as usize, Arc::clone(&self.block_device))
                .lock()
                .modify(root_offset, |root: &mut DiskInode| {
                    let file_count = (root.size as usize) / DIRENT_SZ;
                    let mut found = false;
                    for i in 0..file_count {
                        let mut dirent = DirEntry::empty();
                        if root.read_at(
                            i * DIRENT_SZ,
                            dirent.as_bytes_mut(),
                            &self.block_device,
                        ) == DIRENT_SZ
                            && dirent.name() == name
                        {
                            found = true;
                            let empty = DirEntry::empty();
                            root.write_at(
                                i * DIRENT_SZ,
                                empty.as_bytes(),
                                &self.block_device,
                            );
                            break;
                        }
                    }
                    if !found {
                        return (false, 0);
                    }
                    (true, self.count_links(root))
                })
        };
        if success && remaining == 0 {
            self.clear();
            let mut fs = self.fs.lock();
            fs.dealloc_inode(self.inode_id);
            block_cache_sync_all();
        }
        success
    }
    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over a disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_id() as u32);
            }
        }
        None
    }
    /// Find inode under current inode by name
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    block_id,
                    block_offset,
                    inode_id,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }
    /// Create inode under current inode by name
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        if self.read_disk_inode(op).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = fs.alloc_inode();
        // initialize inode
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });
        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            block_id,
            block_offset,
            new_inode_id,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock automatically by compiler
    }
    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                if !dirent.name().is_empty() {
                    v.push(String::from(dirent.name()));
                }
            }
            v
        })
    }
    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }
}
