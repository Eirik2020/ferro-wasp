use std::{fs, ops::Range, path::Path};

use serde::Serialize;

use crate::{
    error::{FerroError, Result},
    firmware::board::BoardProfile,
};

const MAX_ELF_FILE_SIZE: u64 = 16 * 1024 * 1024;
const ELF32_HEADER_SIZE: usize = 52;
const ELF32_PROGRAM_HEADER_SIZE: usize = 32;
const PT_LOAD: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreparedImage {
    pub board_id: String,
    pub base_address: u32,
    pub entry_address: u32,
    pub initial_stack_pointer: u32,
    pub reset_vector: u32,
    pub source_size: u64,
    pub image_size: usize,
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

pub fn prepare_elf(path: &Path, profile: &BoardProfile) -> Result<PreparedImage> {
    let metadata = fs::metadata(path).map_err(|error| FerroError::InvalidFirmware {
        reason: format!("could not inspect {}: {error}", path.display()),
    })?;
    if metadata.len() > MAX_ELF_FILE_SIZE {
        return invalid(format!(
            "{} is {} bytes; the input limit is {MAX_ELF_FILE_SIZE} bytes",
            path.display(),
            metadata.len()
        ));
    }
    let bytes = fs::read(path).map_err(|error| FerroError::InvalidFirmware {
        reason: format!("could not read {}: {error}", path.display()),
    })?;
    prepare_elf_bytes(&bytes, metadata.len(), profile)
}

fn prepare_elf_bytes(
    elf: &[u8],
    source_size: u64,
    profile: &BoardProfile,
) -> Result<PreparedImage> {
    if elf.len() < ELF32_HEADER_SIZE || &elf[..4] != b"\x7fELF" {
        return invalid("input is not an ELF file".to_owned());
    }
    if elf[4] != 1 {
        return invalid("firmware must be ELF32".to_owned());
    }
    if elf[5] != 1 {
        return invalid("firmware must be little-endian".to_owned());
    }
    if elf[6] != 1 || read_u32(elf, 20)? != 1 {
        return invalid("ELF version is unsupported".to_owned());
    }
    if read_u16(elf, 16)? != 2 {
        return invalid("ELF must be an executable image".to_owned());
    }
    if read_u16(elf, 18)? != 40 {
        return invalid("ELF machine is not ARM".to_owned());
    }
    if read_u16(elf, 40)? as usize != ELF32_HEADER_SIZE {
        return invalid("ELF header has an unexpected size".to_owned());
    }

    let entry_address = read_u32(elf, 24)?;
    if entry_address & 1 == 0 || !profile.flash_range.contains(&(entry_address & !1)) {
        return invalid(format!(
            "ELF entry point {entry_address:#010x} is not a Thumb address inside board flash"
        ));
    }
    let program_offset = read_u32(elf, 28)? as usize;
    let program_entry_size = read_u16(elf, 42)? as usize;
    let program_count = read_u16(elf, 44)? as usize;
    if program_entry_size < ELF32_PROGRAM_HEADER_SIZE {
        return invalid("ELF program-header entries are too small".to_owned());
    }
    let table_size = program_entry_size
        .checked_mul(program_count)
        .ok_or_else(|| invalid_error("ELF program-header table overflows"))?;
    checked_slice(elf, program_offset, table_size)?;

    struct Segment<'a> {
        address: u32,
        data: &'a [u8],
    }
    let mut segments = Vec::new();
    for index in 0..program_count {
        let offset = program_offset
            .checked_add(
                index
                    .checked_mul(program_entry_size)
                    .ok_or_else(|| invalid_error("ELF program-header index overflows"))?,
            )
            .ok_or_else(|| invalid_error("ELF program-header offset overflows"))?;
        let header = checked_slice(elf, offset, ELF32_PROGRAM_HEADER_SIZE)?;
        if read_u32(header, 0)? != PT_LOAD {
            continue;
        }
        let file_offset = read_u32(header, 4)? as usize;
        let virtual_address = read_u32(header, 8)?;
        let physical_address = read_u32(header, 12)?;
        let file_size = read_u32(header, 16)? as usize;
        let memory_size = read_u32(header, 20)? as usize;
        if memory_size < file_size {
            return invalid(format!(
                "load segment {index} has p_memsz smaller than p_filesz"
            ));
        }
        if file_size == 0 {
            continue;
        }
        let data = checked_slice(elf, file_offset, file_size)?;
        let address = valid_load_address(physical_address, file_size, &profile.flash_range)
            .or_else(|| valid_load_address(virtual_address, file_size, &profile.flash_range))
            .ok_or_else(|| {
                invalid_error(&format!(
                    "load segment {index} is outside {} flash ({physical_address:#010x}/{virtual_address:#010x}, {file_size} bytes)",
                    profile.display_name
                ))
            })?;
        segments.push(Segment { address, data });
    }
    if segments.is_empty() {
        return invalid("ELF contains no file-backed loadable flash segments".to_owned());
    }

    let base_address = segments
        .iter()
        .map(|segment| segment.address)
        .min()
        .ok_or_else(|| invalid_error("ELF has no load address"))?;
    let mut end_address = base_address;
    for segment in &segments {
        let length = u32::try_from(segment.data.len())
            .map_err(|_| invalid_error("load segment is too large"))?;
        let end = segment
            .address
            .checked_add(length)
            .ok_or_else(|| invalid_error("load segment address overflows"))?;
        end_address = end_address.max(end);
    }
    if base_address != profile.flash_range.start {
        return invalid(format!(
            "image starts at {base_address:#010x}; {} requires {:#010x}",
            profile.display_name, profile.flash_range.start
        ));
    }
    let dense_len = usize::try_from(end_address - base_address)
        .map_err(|_| invalid_error("dense image length is unsupported"))?;
    let mut image = vec![0xff; dense_len];
    let mut occupied = vec![false; dense_len];
    for segment in segments {
        let start = usize::try_from(segment.address - base_address)
            .map_err(|_| invalid_error("segment offset is unsupported"))?;
        for (index, byte) in segment.data.iter().copied().enumerate() {
            let destination = start
                .checked_add(index)
                .ok_or_else(|| invalid_error("dense image index overflows"))?;
            if occupied[destination] && image[destination] != byte {
                return invalid(format!(
                    "load segments overlap with conflicting data at {:#010x}",
                    base_address + destination as u32
                ));
            }
            image[destination] = byte;
            occupied[destination] = true;
        }
    }
    if image.len() < 8 || !occupied[..8].iter().all(|value| *value) {
        return invalid("image does not contain a complete vector table at flash base".to_owned());
    }
    let initial_stack_pointer = u32::from_le_bytes([image[0], image[1], image[2], image[3]]);
    let reset_vector = u32::from_le_bytes([image[4], image[5], image[6], image[7]]);
    if initial_stack_pointer % 8 != 0
        || initial_stack_pointer < profile.ram_range.start
        || initial_stack_pointer > profile.ram_range.end
    {
        return invalid(format!(
            "initial stack pointer {initial_stack_pointer:#010x} is not 8-byte aligned inside board RAM"
        ));
    }
    if reset_vector & 1 == 0 {
        return invalid(format!(
            "reset vector {reset_vector:#010x} does not set the Thumb bit"
        ));
    }
    let reset_address = reset_vector & !1;
    if !profile.flash_range.contains(&reset_address) {
        return invalid(format!(
            "reset vector {reset_vector:#010x} is outside board application flash"
        ));
    }

    Ok(PreparedImage {
        board_id: profile.id.to_owned(),
        base_address,
        entry_address,
        initial_stack_pointer,
        reset_vector,
        source_size,
        image_size: image.len(),
        bytes: image,
    })
}

fn valid_load_address(address: u32, len: usize, range: &Range<u32>) -> Option<u32> {
    let len = u32::try_from(len).ok()?;
    let end = address.checked_add(len)?;
    (address >= range.start && end <= range.end).then_some(address)
}

fn checked_slice(bytes: &[u8], offset: usize, len: usize) -> Result<&[u8]> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| invalid_error("ELF file range overflows"))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| invalid_error("ELF references bytes outside the file"))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let value = checked_slice(bytes, offset, 2)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let value = checked_slice(bytes, offset, 4)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn invalid<T>(reason: String) -> Result<T> {
    Err(invalid_error(&reason))
}

fn invalid_error(reason: &str) -> FerroError {
    FerroError::InvalidFirmware {
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elf_with_segments(segments: &[(u32, u32, Vec<u8>, u32)]) -> Vec<u8> {
        let headers_end = ELF32_HEADER_SIZE + ELF32_PROGRAM_HEADER_SIZE * segments.len();
        let data_len: usize = segments.iter().map(|(_, _, data, _)| data.len()).sum();
        let mut elf = vec![0u8; headers_end + data_len];
        elf[..4].copy_from_slice(b"\x7fELF");
        elf[4] = 1;
        elf[5] = 1;
        elf[6] = 1;
        put_u16(&mut elf, 16, 2);
        put_u16(&mut elf, 18, 40);
        put_u32(&mut elf, 20, 1);
        put_u32(&mut elf, 24, 0x0800_0101);
        put_u32(&mut elf, 28, ELF32_HEADER_SIZE as u32);
        put_u16(&mut elf, 40, ELF32_HEADER_SIZE as u16);
        put_u16(&mut elf, 42, ELF32_PROGRAM_HEADER_SIZE as u16);
        put_u16(&mut elf, 44, segments.len() as u16);
        let mut data_offset = headers_end;
        for (index, (virtual_address, physical_address, data, memory_size)) in
            segments.iter().enumerate()
        {
            let header = ELF32_HEADER_SIZE + index * ELF32_PROGRAM_HEADER_SIZE;
            put_u32(&mut elf, header, PT_LOAD);
            put_u32(&mut elf, header + 4, data_offset as u32);
            put_u32(&mut elf, header + 8, *virtual_address);
            put_u32(&mut elf, header + 12, *physical_address);
            put_u32(&mut elf, header + 16, data.len() as u32);
            put_u32(&mut elf, header + 20, *memory_size);
            elf[data_offset..data_offset + data.len()].copy_from_slice(data);
            data_offset += data.len();
        }
        elf
    }

    fn vector_data(extra: usize) -> Vec<u8> {
        let mut data = vec![0xaa; 8 + extra];
        data[..4].copy_from_slice(&0x2002_0000u32.to_le_bytes());
        data[4..8].copy_from_slice(&0x0800_0101u32.to_le_bytes());
        data
    }

    fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn valid_arm_elf_uses_physical_flash_address_and_omits_bss() {
        let elf = elf_with_segments(&[(0x2000_0000, 0x0800_0000, vector_data(8), 128)]);
        let image =
            prepare_elf_bytes(&elf, elf.len() as u64, &BoardProfile::FOXEER_F405_V2).unwrap();
        assert_eq!(image.base_address, 0x0800_0000);
        assert_eq!(image.image_size, 16);
    }

    #[test]
    fn gaps_are_filled_and_matching_overlap_is_accepted() {
        let elf = elf_with_segments(&[
            (0x0800_0000, 0x0800_0000, vector_data(0), 8),
            (
                0x0800_0004,
                0x0800_0004,
                0x0800_0101u32.to_le_bytes().to_vec(),
                4,
            ),
            (0x0800_0010, 0x0800_0010, vec![0x55], 1),
        ]);
        let image = prepare_elf_bytes(&elf, 0, &BoardProfile::FOXEER_F405_V2).unwrap();
        assert_eq!(&image.bytes[8..16], &[0xff; 8]);
        assert_eq!(image.bytes[16], 0x55);
    }

    #[test]
    fn rejects_wrong_architecture_ranges_and_vectors() {
        let mut wrong_arch = elf_with_segments(&[(0x0800_0000, 0x0800_0000, vector_data(0), 8)]);
        put_u16(&mut wrong_arch, 18, 62);
        assert!(prepare_elf_bytes(&wrong_arch, 0, &BoardProfile::FOXEER_F405_V2).is_err());

        let out_of_range = elf_with_segments(&[(0x0900_0000, 0x0900_0000, vector_data(0), 8)]);
        assert!(prepare_elf_bytes(&out_of_range, 0, &BoardProfile::FOXEER_F405_V2).is_err());

        let mut bad_vector = vector_data(0);
        bad_vector[4..8].copy_from_slice(&0x0800_0100u32.to_le_bytes());
        let elf = elf_with_segments(&[(0x0800_0000, 0x0800_0000, bad_vector, 8)]);
        assert!(prepare_elf_bytes(&elf, 0, &BoardProfile::FOXEER_F405_V2).is_err());

        let mut bad_entry = elf_with_segments(&[(0x0800_0000, 0x0800_0000, vector_data(0), 8)]);
        put_u32(&mut bad_entry, 24, 0x0800_0100);
        assert!(prepare_elf_bytes(&bad_entry, 0, &BoardProfile::FOXEER_F405_V2).is_err());
    }

    #[test]
    fn conflicting_overlap_is_rejected() {
        let elf = elf_with_segments(&[
            (0x0800_0000, 0x0800_0000, vector_data(0), 8),
            (0x0800_0004, 0x0800_0004, vec![0, 0, 0, 0], 4),
        ]);
        assert!(prepare_elf_bytes(&elf, 0, &BoardProfile::FOXEER_F405_V2).is_err());
    }

    #[test]
    fn truncated_and_overflowing_headers_are_rejected_without_panics() {
        assert!(prepare_elf_bytes(b"\x7fELF", 4, &BoardProfile::FOXEER_F405_V2).is_err());
        let mut elf = elf_with_segments(&[(0x0800_0000, 0x0800_0000, vector_data(0), 8)]);
        put_u32(&mut elf, 28, u32::MAX);
        assert!(prepare_elf_bytes(&elf, 0, &BoardProfile::FOXEER_F405_V2).is_err());
    }
}
