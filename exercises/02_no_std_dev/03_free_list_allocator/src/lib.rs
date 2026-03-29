//! # Free-List Allocator
//!
//! Building on the bump allocator, implement a Free-List Allocator that supports memory reclamation.
//!
//! ## How It Works
//!
//! A Free-List Allocator uses a linked list to track all freed memory blocks.
//! On allocation, it first searches the list for a suitable block (first-fit strategy);
//! if none is found, it falls back to allocating from the unused region.
//! On deallocation, the block is inserted at the head of the list.
//!
//! ```text
//! free_list -> [block A: 64B] -> [block B: 128B] -> [block C: 32B] -> null
//! ```
//!
//! Each free block stores a `FreeBlock` struct at its head (containing block size and next pointer).
//!
//! ## Task
//!
//! Implement `FreeListAllocator`'s `alloc` and `dealloc` methods:
//!
//! ### alloc
//! 1. Traverse the free_list, find the first block with `size >= layout.size()` and proper alignment (first-fit)
//! 2. If found, remove it from the list and return it
//! 3. If not found, allocate from the `bump` region (same as bump allocator)
//!
//! ### dealloc
//! 1. Write `FreeBlock` header info at the freed block
//! 2. Insert it at the head of free_list
//!
//! ## Key Concepts
//!
//! - Intrusive linked list
//! - `*mut T` read/write: `ptr.write(val)` / `ptr.read()`
//! - Memory alignment checks

#![cfg_attr(not(test), no_std)]

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;

/// Free block header, stored at the beginning of each free memory block
struct FreeBlock {
    size: usize,
    next: *mut FreeBlock,
}

pub struct FreeListAllocator {
    heap_start: usize,
    heap_end: usize,
    /// Bump pointer: unallocated region starts here
    bump_next: core::sync::atomic::AtomicUsize,
    /// Free list head (protected by Mutex in test, UnsafeCell otherwise)
    #[cfg(test)]
    free_list: std::sync::Mutex<*mut FreeBlock>,
    #[cfg(not(test))]
    free_list: core::cell::UnsafeCell<*mut FreeBlock>,
}

#[cfg(test)]
unsafe impl Send for FreeListAllocator {}
#[cfg(test)]
unsafe impl Sync for FreeListAllocator {}
#[cfg(not(test))]
unsafe impl Send for FreeListAllocator {}
#[cfg(not(test))]
unsafe impl Sync for FreeListAllocator {}

impl FreeListAllocator {
    /// # Safety
    /// `heap_start..heap_end` must be a valid readable and writable memory region.
    pub unsafe fn new(heap_start: usize, heap_end: usize) -> Self {
        Self {
            heap_start,
            heap_end,
            bump_next: core::sync::atomic::AtomicUsize::new(heap_start),
            #[cfg(test)]
            free_list: std::sync::Mutex::new(null_mut()),
            #[cfg(not(test))]
            free_list: core::cell::UnsafeCell::new(null_mut()),
        }
    }

    #[cfg(test)]
    fn free_list_head(&self) -> *mut FreeBlock {
        *self.free_list.lock().unwrap()
    }

    #[cfg(test)]
    fn set_free_list_head(&self, head: *mut FreeBlock) {
        *self.free_list.lock().unwrap() = head;
    }

    #[cfg(not(test))]
    fn free_list_head(&self) -> *mut FreeBlock {
        unsafe { *self.free_list.get() }
    }

    #[cfg(not(test))]
    fn set_free_list_head(&self, head: *mut FreeBlock) {
        unsafe { *self.free_list.get() = head }
    }
}

unsafe impl GlobalAlloc for FreeListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Ensure block is at least large enough to hold a FreeBlock header (for future dealloc)
        let size = layout.size().max(core::mem::size_of::<FreeBlock>());
        let align = layout.align().max(core::mem::align_of::<FreeBlock>());

        // TODO: Step 1 — traverse free_list, find a suitable block (first-fit)
        //
        // Hints:
        // - Use prev_ptr and curr to traverse the list
        // - Check if curr address satisfies align, and (*curr).size >= size
        // - If found, remove it from the list (update prev's next or the free_list head)
        // - Return curr as *mut u8

        // TODO: Step 2 — no suitable block in free_list, allocate from bump region
        //
        // Same logic as 02_bump_allocator's alloc
		        // Step 1: traverse free_list, find a suitable block (first-fit)
        let mut prev_ptr: *mut FreeBlock = null_mut();
        let mut curr = self.free_list_head();
        
        while !curr.is_null() {
            let block = &*curr;
            
            // Check if current block meets size and alignment requirements
            let block_addr = curr as usize;
            let aligned_addr = block_addr.checked_add(align - 1)
                .map(|x| x & !(align - 1))
                .unwrap_or(0);
            
            let padding = aligned_addr - block_addr;
            
            if block.size >= size + padding {
                
                if prev_ptr.is_null() {
                    self.set_free_list_head(block.next);
                } else {
                    (*prev_ptr).next = block.next;
                }
                
                // Calculate the actual allocation address
                let alloc_addr = if padding > 0 {
                    // Need to split off the padding area
                    // If padding is large enough, we can create a new free block
                    if padding >= core::mem::size_of::<FreeBlock>() {
                        // Create a free block for the padding area
                        let padding_block = curr as usize;
                        (padding_block as *mut FreeBlock).write(FreeBlock {
                            size: padding,
                            next: curr, // This will be fixed below
                        });
                        
                        let new_block_addr = padding_block + padding;
                        let new_block = new_block_addr as *mut FreeBlock;
                        
                        new_block.write(FreeBlock {
                            size: block.size - padding,
                            next: block.next,
                        });
                        
                        let padding_block_ptr = padding_block as *mut FreeBlock;
                        if prev_ptr.is_null() {
                            self.set_free_list_head(padding_block_ptr);
                        } else {
                            (*prev_ptr).next = padding_block_ptr;
                        }
                        padding_block_ptr.write(FreeBlock {
                            size: padding,
                            next: new_block,
                        });
                        
                        return aligned_addr as *mut u8;
                    }
                    aligned_addr as *mut u8
                } else {
                    curr as *mut u8
                };
                
                // Check if we need to split off excess space
                let remaining = block.size - size - padding;
                if remaining >= core::mem::size_of::<FreeBlock>() {
                    // Split: create a new free block after allocation
                    let alloc_end = alloc_addr as usize + size;
                    let remaining_block = alloc_end as *mut FreeBlock;
                    remaining_block.write(FreeBlock {
                        size: remaining,
                        next: block.next,
                    });
                    
                    // Update the free list to include the remaining block
                    if prev_ptr.is_null() {
                        self.set_free_list_head(remaining_block);
                    } else {
                        (*prev_ptr).next = remaining_block;
                    }
                }
                
                return alloc_addr as *mut u8;
            }
            
            prev_ptr = curr;
            curr = block.next;
        }

        // Step 2: no suitable block in free_list, allocate from bump region
        
        let bump_next = self.bump_next.load(core::sync::atomic::Ordering::Relaxed);
        
        // Align the bump pointer
        let aligned_bump = bump_next.checked_add(align - 1)
            .map(|x| x & !(align - 1))
            .unwrap_or(bump_next); // 如果对齐计算失败，使用原地址
        
        // 检查加法是否溢出
        let new_bump = match aligned_bump.checked_add(size) {
            Some(v) => v,
            None => return null_mut(), // 溢出，分配失败
        };
        
        // Check if we have enough space
        if new_bump > self.heap_end {
            return null_mut(); // Out of memory
        }
        
        // Update bump pointer
        self.bump_next.store(new_bump, core::sync::atomic::Ordering::Relaxed);
        
        // Return the aligned pointer
        aligned_bump as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size().max(core::mem::size_of::<FreeBlock>());

        // TODO: Insert the freed block at the head of free_list
        //
        // Steps:
        // 1. Cast ptr to *mut FreeBlock
        // 2. Write FreeBlock { size, next: current list head }
        // 3. Update free_list head to ptr

		// Insert the freed block at the head of free_list
		
		// 1. Cast ptr to *mut FreeBlock
		let free_block = ptr as *mut FreeBlock;
		
		// 2. Write FreeBlock { size, next: current list head }
		// 获取当前空闲链表头
		let current_head = self.free_list_head();
		
		// 在释放的内存块头部写入 FreeBlock 结构
		free_block.write(FreeBlock {
			size,
			next: current_head,
    });
    
    // 3. Update free_list head to ptr
    self.set_free_list_head(free_block);
    }
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    const HEAP_SIZE: usize = 4096;

    fn make_allocator() -> (FreeListAllocator, Vec<u8>) {
        let mut heap = vec![0u8; HEAP_SIZE];
        let start = heap.as_mut_ptr() as usize;
        let alloc = unsafe { FreeListAllocator::new(start, start + HEAP_SIZE) };
        (alloc, heap)
    }

    #[test]
    fn test_alloc_basic() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(32, 8).unwrap();
        let ptr = unsafe { alloc.alloc(layout) };
        assert!(!ptr.is_null());
    }

    #[test]
    fn test_alloc_alignment() {
        let (alloc, _heap) = make_allocator();
        for align in [1, 2, 4, 8, 16] {
            let layout = Layout::from_size_align(8, align).unwrap();
            let ptr = unsafe { alloc.alloc(layout) };
            assert!(!ptr.is_null());
            assert_eq!(ptr as usize % align, 0, "align={align}");
        }
    }

    #[test]
    fn test_dealloc_and_reuse() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(64, 8).unwrap();

        let p1 = unsafe { alloc.alloc(layout) };
        assert!(!p1.is_null());

        // After freeing, the next allocation should reuse the same block
        unsafe { alloc.dealloc(p1, layout) };
        let p2 = unsafe { alloc.alloc(layout) };
        assert!(!p2.is_null());
        assert_eq!(p1, p2, "should reuse the freed block");
    }

    #[test]
    fn test_multiple_alloc_dealloc() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(128, 8).unwrap();

        let p1 = unsafe { alloc.alloc(layout) };
        let p2 = unsafe { alloc.alloc(layout) };
        let p3 = unsafe { alloc.alloc(layout) };
        assert!(!p1.is_null() && !p2.is_null() && !p3.is_null());

        unsafe { alloc.dealloc(p2, layout) };
        unsafe { alloc.dealloc(p1, layout) };

        let q1 = unsafe { alloc.alloc(layout) };
        let q2 = unsafe { alloc.alloc(layout) };
        assert!(!q1.is_null() && !q2.is_null());
    }

    #[test]
    fn test_oom() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(HEAP_SIZE + 1, 1).unwrap();
        let ptr = unsafe { alloc.alloc(layout) };
        assert!(ptr.is_null(), "should return null when exceeding heap");
    }
}
