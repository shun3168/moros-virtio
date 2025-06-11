use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{
    page::PageRangeInclusive,
    OffsetPageTable, PageTable, PhysFrame, Size4KiB,
    Page, PageTableFlags, Mapper, FrameAllocator,
    PageSize,
};

// Modified by shshi102
use x86_64::{VirtAddr, PhysAddr};
use x86_64::structures::paging::mapper::MapToError;

pub unsafe fn active_page_table() -> &'static mut PageTable {
    let (frame, _) = Cr3::read();
    let phys_addr = frame.start_address();
    let virt_addr = super::phys_to_virt(phys_addr);
    let page_table_ptr: *mut PageTable = virt_addr.as_mut_ptr();
    &mut *page_table_ptr // unsafe
}

pub unsafe fn create_page_table(frame: PhysFrame) -> &'static mut PageTable {
    let phys_addr = frame.start_address();
    let virt_addr = super::phys_to_virt(phys_addr);
    let page_table_ptr: *mut PageTable = virt_addr.as_mut_ptr();
    &mut *page_table_ptr // unsafe
}

// Modified by shshi102
pub fn alloc_pages(
    mapper: &mut OffsetPageTable,
    addr: u64,
    size: usize,
) -> Result<(), ()> {
    let size = size.saturating_sub(1) as u64;
    let mut frame_allocator_ref = super::frame_allocator();

    let pages = {
        let start_page = Page::containing_address(VirtAddr::new(addr));
        let end_page = Page::containing_address(VirtAddr::new(addr + size));
        Page::range_inclusive(start_page, end_page)
    };

    let flags = PageTableFlags::PRESENT
                  | PageTableFlags::WRITABLE
                  | PageTableFlags::USER_ACCESSIBLE;

    for page in pages {
        if let Some(frame) = frame_allocator_ref.allocate_frame() {
            let res = unsafe {
                mapper.map_to(page, frame, flags, &mut frame_allocator_ref)
            };
            if let Ok(mapping) = res {
                mapping.flush();
            } else {
                debug!("Could not map {:?} to {:?}: {:?}", page, frame, res.unwrap_err());
                if let Ok(old_frame) = mapper.translate_page(page) {
                    debug!("Already mapped to {:?}", old_frame);
                }
                return Err(());
            }
        } else {
            debug!("Could not allocate frame for {:?}", page);
            return Err(());
        }
    }

    Ok(())
}

// TODO: Replace `free` by `dealloc`
pub fn free_pages(mapper: &mut OffsetPageTable, addr: u64, size: usize) {
    let size = size.saturating_sub(1) as u64;

    let pages: PageRangeInclusive<Size4KiB> = {
        let start_page = Page::containing_address(VirtAddr::new(addr));
        let end_page = Page::containing_address(VirtAddr::new(addr + size));
        Page::range_inclusive(start_page, end_page)
    };

    for page in pages {
        if let Ok((_, mapping)) = mapper.unmap(page) {
            mapping.flush();
        } else {
            //debug!("Could not unmap {:?}", page);
        }
    }
}


// Added to create physically contiguous region for DMA for Framebuffer, Modified by shshi102
pub unsafe fn map_contiguous_physical_region(
    mapper: &mut OffsetPageTable<'static>,
    phys_start: PhysAddr,
    virt_start: VirtAddr,
    size: usize,
) -> Result<(), MapToError<Size4KiB>> { // Changed return type to MapToError
    debug_assert!(phys_start.is_aligned(Size4KiB::SIZE));
    debug_assert!(virt_start.is_aligned(Size4KiB::SIZE));

    let mut current_phys_addr = phys_start;
    let mut current_virt_addr = virt_start;
    let mut frame_allocator_ref = super::frame_allocator();

    let num_pages = (size + Size4KiB::SIZE as usize - 1) / Size4KiB::SIZE as usize;

    for _i in 0..num_pages {
        let phys_frame = PhysFrame::<Size4KiB>::containing_address(current_phys_addr);
        let virt_page = Page::containing_address(current_virt_addr);

        let flags = PageTableFlags::PRESENT
                  | PageTableFlags::WRITABLE
                  | PageTableFlags::NO_CACHE
                  | PageTableFlags::ACCESSED
                  | PageTableFlags::GLOBAL;

        mapper.map_to(virt_page, phys_frame, flags, &mut frame_allocator_ref)
            .map_err(|e| e)? // Changed error mapping to propagate MapToError
            .flush();

        current_phys_addr += Size4KiB::SIZE;
        current_virt_addr += Size4KiB::SIZE;
    }
    Ok(())
}