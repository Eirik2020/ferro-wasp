//! Reusable blackbox catalogue and download operations.

use crate::flash_storage as flash_task;
use ferrowasp_mspv2 as mspv2;

pub trait FlashRead {
    fn read(&mut self, address: u32, output: &mut [u8]) -> Result<(), ()>;
}

impl<F> FlashRead for F
where
    F: FnMut(u32, &mut [u8]) -> Result<(), ()>,
{
    fn read(&mut self, address: u32, output: &mut [u8]) -> Result<(), ()> {
        self(address, output)
    }
}

pub fn blackbox_flight_id_at(
    flash: &mut impl FlashRead,
    layout: flash_task::StorageLayout,
    page_index: u32,
) -> Result<u32, ()> {
    let address = layout.log_page_address(page_index).ok_or(())?;
    let mut page = [0xff; ferrowasp_core::blackbox::FLASH_PAGE_LEN];
    flash.read(address, &mut page).map_err(|_| ())?;
    ferrowasp_core::blackbox::decode_page(&page)
        .map(|metadata| metadata.flight_id)
        .map_err(|_| ())
}

pub fn blackbox_bounds(
    flash: &mut impl FlashRead,
    layout: flash_task::StorageLayout,
    used_pages: u32,
    id: mspv2::rpc::BlackboxId,
) -> Result<Option<(u32, u32)>, ()> {
    let mut low = 0;
    let mut high = used_pages;
    while low < high {
        let middle = low + (high - low) / 2;
        if blackbox_flight_id_at(flash, layout, middle)? < id.0 {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let start = low;
    if start == used_pages || blackbox_flight_id_at(flash, layout, start)? != id.0 {
        return Ok(None);
    }
    high = used_pages;
    while low < high {
        let middle = low + (high - low) / 2;
        if blackbox_flight_id_at(flash, layout, middle)? <= id.0 {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    Ok(Some((start, low)))
}

pub fn blackbox_info(
    flash: &mut impl FlashRead,
    layout: flash_task::StorageLayout,
    used_pages: u32,
    id: mspv2::rpc::BlackboxId,
    active_id: Option<u32>,
) -> Result<Option<mspv2::rpc::BlackboxInfo>, ()> {
    let Some((start, end)) = blackbox_bounds(flash, layout, used_pages, id)? else {
        return Ok(None);
    };
    Ok(Some(mspv2::rpc::BlackboxInfo {
        id,
        size_bytes: (end - start) * ferrowasp_core::blackbox::FLASH_PAGE_LEN as u32,
        state: if active_id == Some(id.0) {
            mspv2::rpc::BlackboxState::Active
        } else {
            mspv2::rpc::BlackboxState::Complete
        },
        created_unix_s: None,
        file_crc32: None,
    }))
}

pub fn list_blackboxes(
    flash: &mut impl FlashRead,
    layout: flash_task::StorageLayout,
    used_pages: u32,
    active_id: Option<u32>,
) -> Result<mspv2::rpc::BlackboxList, ()> {
    let mut entries = heapless::Vec::new();
    if used_pages == 0 {
        return Ok(mspv2::rpc::BlackboxList {
            entries,
            truncated: false,
        });
    }
    let mut cursor = used_pages;
    while cursor != 0 {
        let id = mspv2::rpc::BlackboxId(blackbox_flight_id_at(flash, layout, cursor - 1)?);
        let Some((start, end)) = blackbox_bounds(flash, layout, used_pages, id)? else {
            return Err(());
        };
        let info = mspv2::rpc::BlackboxInfo {
            id,
            size_bytes: (end - start) * ferrowasp_core::blackbox::FLASH_PAGE_LEN as u32,
            state: if active_id == Some(id.0) {
                mspv2::rpc::BlackboxState::Active
            } else {
                mspv2::rpc::BlackboxState::Complete
            },
            created_unix_s: None,
            file_crc32: None,
        };
        if entries.push(info).is_err() {
            break;
        }
        cursor = start;
    }
    Ok(mspv2::rpc::BlackboxList {
        entries,
        truncated: cursor != 0,
    })
}

pub fn read_blackbox_chunk(
    flash: &mut impl FlashRead,
    layout: flash_task::StorageLayout,
    used_pages: u32,
    id: mspv2::rpc::BlackboxId,
    offset: u32,
    requested_length: u16,
) -> Result<mspv2::rpc::BlackboxChunk, mspv2::rpc::DeviceError> {
    let (start, end) = blackbox_bounds(flash, layout, used_pages, id)
        .map_err(|_| mspv2::rpc::DeviceError::ReadFailure)?
        .ok_or(mspv2::rpc::DeviceError::BlackboxNotFound)?;
    let size = (end - start) * ferrowasp_core::blackbox::FLASH_PAGE_LEN as u32;
    if offset > size || requested_length == 0 {
        return Err(mspv2::rpc::DeviceError::InvalidOffset);
    }
    let requested = usize::from(requested_length).min(mspv2::rpc::MAX_BLACKBOX_CHUNK);
    let actual = requested.min((size - offset) as usize);
    let mut data = heapless::Vec::<u8, { mspv2::rpc::MAX_BLACKBOX_CHUNK }>::new();
    let mut position = offset as usize;
    while data.len() < actual {
        let relative_page = position / ferrowasp_core::blackbox::FLASH_PAGE_LEN;
        let page_offset = position % ferrowasp_core::blackbox::FLASH_PAGE_LEN;
        let page_index = start + relative_page as u32;
        let address = layout
            .log_page_address(page_index)
            .ok_or(mspv2::rpc::DeviceError::ReadFailure)?;
        let mut page = [0xff; ferrowasp_core::blackbox::FLASH_PAGE_LEN];
        flash
            .read(address, &mut page)
            .map_err(|_| mspv2::rpc::DeviceError::ReadFailure)?;
        let metadata = ferrowasp_core::blackbox::decode_page(&page)
            .map_err(|_| mspv2::rpc::DeviceError::ChecksumMismatch)?;
        if metadata.flight_id != id.0 || page_index >= end {
            return Err(mspv2::rpc::DeviceError::ReadFailure);
        }
        let copy_len =
            (ferrowasp_core::blackbox::FLASH_PAGE_LEN - page_offset).min(actual - data.len());
        data.extend_from_slice(&page[page_offset..page_offset + copy_len])
            .map_err(|_| mspv2::rpc::DeviceError::Internal)?;
        position += copy_len;
    }
    let chunk_crc32 = ferrowasp_core::blackbox::crc32(data.as_slice());
    Ok(mspv2::rpc::BlackboxChunk {
        id,
        offset,
        data,
        chunk_crc32,
        end_of_file: offset.saturating_add(actual as u32) == size,
    })
}
