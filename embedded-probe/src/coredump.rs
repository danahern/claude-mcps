//! ELF core dump generation for ARM Cortex-M targets.
//!
//! Produces GDB-compatible ELF core files with NT_PRSTATUS notes
//! and PT_LOAD segments for RAM regions.

use std::io::Write;

// ELF constants
const ELFMAG: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS32: u8 = 1;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_CORE: u16 = 4;
const EM_ARM: u16 = 40;
const PT_NOTE: u32 = 4;
const PT_LOAD: u32 = 1;
const PF_R: u32 = 4;
const PF_W: u32 = 2;

// ELF32 structure sizes
const ELF32_EHDR_SIZE: u16 = 52;
const ELF32_PHDR_SIZE: u16 = 32;

// Note types
const NT_PRSTATUS: u32 = 1;

/// Register indices in the 17-element array: R0-R12, SP, LR, PC, xPSR
/// GDB ARM expects registers in order: R0-R12, SP, LR, PC, xPSR
/// (matching the ARM PRSTATUS layout)
const NUM_REGS: usize = 17;

/// Size of the prstatus note descriptor for ARM:
/// signal number (4) + padding (8) + pid/pgrp/sid (12) + times (32) + regs (17*4=68) = 124
/// Simplified: we use just the register portion at offset 72
const PRSTATUS_SIZE: usize = 148;

/// Write an ELF core dump to the given writer.
///
/// `registers`: R0-R12, SP, LR, PC, xPSR (17 values)
/// `memory_regions`: (base_address, data) pairs for RAM regions
pub fn write_elf_coredump(
    output: &mut impl Write,
    registers: &[u32; NUM_REGS],
    memory_regions: &[(u64, &[u8])],
) -> std::io::Result<()> {
    // Calculate layout
    let num_phdrs = 1 + memory_regions.len(); // PT_NOTE + PT_LOAD per region
    let phdr_offset = ELF32_EHDR_SIZE as u32;
    let phdr_total_size = num_phdrs as u32 * ELF32_PHDR_SIZE as u32;

    // Note segment: name "CORE\0" (padded to 8), type NT_PRSTATUS, descriptor = prstatus
    let note_name = b"CORE\0\0\0\0"; // 5 bytes + 3 padding to align to 4
    let note_namesz: u32 = 5; // "CORE\0"
    let note_descsz: u32 = PRSTATUS_SIZE as u32;
    // Note header: namesz(4) + descsz(4) + type(4) + name(8) + desc(aligned)
    let note_header_size: u32 = 12;
    let note_total_size = note_header_size + 8 + align4(note_descsz);

    let note_offset = phdr_offset + phdr_total_size;
    let mut data_offset = note_offset + note_total_size;

    // Write ELF header
    output.write_all(&ELFMAG)?;
    output.write_all(&[ELFCLASS32])?; // EI_CLASS
    output.write_all(&[ELFDATA2LSB])?; // EI_DATA
    output.write_all(&[EV_CURRENT])?; // EI_VERSION
    output.write_all(&[0; 9])?; // EI_OSABI through EI_PAD
    output.write_all(&ET_CORE.to_le_bytes())?; // e_type
    output.write_all(&EM_ARM.to_le_bytes())?; // e_machine
    output.write_all(&1u32.to_le_bytes())?; // e_version
    output.write_all(&0u32.to_le_bytes())?; // e_entry
    output.write_all(&phdr_offset.to_le_bytes())?; // e_phoff
    output.write_all(&0u32.to_le_bytes())?; // e_shoff
    output.write_all(&0u32.to_le_bytes())?; // e_flags
    output.write_all(&ELF32_EHDR_SIZE.to_le_bytes())?; // e_ehsize
    output.write_all(&ELF32_PHDR_SIZE.to_le_bytes())?; // e_phentsize
    output.write_all(&(num_phdrs as u16).to_le_bytes())?; // e_phnum
    output.write_all(&0u16.to_le_bytes())?; // e_shentsize
    output.write_all(&0u16.to_le_bytes())?; // e_shnum
    output.write_all(&0u16.to_le_bytes())?; // e_shstrndx

    // Write PT_NOTE program header
    output.write_all(&PT_NOTE.to_le_bytes())?; // p_type
    output.write_all(&note_offset.to_le_bytes())?; // p_offset
    output.write_all(&0u32.to_le_bytes())?; // p_vaddr
    output.write_all(&0u32.to_le_bytes())?; // p_paddr
    output.write_all(&note_total_size.to_le_bytes())?; // p_filesz
    output.write_all(&note_total_size.to_le_bytes())?; // p_memsz
    output.write_all(&0u32.to_le_bytes())?; // p_flags
    output.write_all(&4u32.to_le_bytes())?; // p_align

    // Write PT_LOAD program headers for each memory region
    for (base_addr, region_data) in memory_regions {
        let region_size = region_data.len() as u32;
        output.write_all(&PT_LOAD.to_le_bytes())?; // p_type
        output.write_all(&data_offset.to_le_bytes())?; // p_offset
        output.write_all(&(*base_addr as u32).to_le_bytes())?; // p_vaddr
        output.write_all(&(*base_addr as u32).to_le_bytes())?; // p_paddr
        output.write_all(&region_size.to_le_bytes())?; // p_filesz
        output.write_all(&region_size.to_le_bytes())?; // p_memsz
        output.write_all(&(PF_R | PF_W).to_le_bytes())?; // p_flags
        output.write_all(&4u32.to_le_bytes())?; // p_align
        data_offset += region_size;
    }

    // Write note segment
    output.write_all(&note_namesz.to_le_bytes())?; // namesz
    output.write_all(&note_descsz.to_le_bytes())?; // descsz
    output.write_all(&NT_PRSTATUS.to_le_bytes())?; // type
    output.write_all(note_name)?; // name with padding

    // Write prstatus descriptor
    // Layout: signal(4) + padding(8) + pid/pgrp/sid(12) + user times(32) = 56 bytes of header
    // Then: 18 registers * 4 bytes = 72 bytes (GDB ARM expects 18 regs: r0-r12, sp, lr, pc, cpsr, ORIG_R0)
    // Total prstatus for ARM = 148 bytes
    let mut prstatus = vec![0u8; PRSTATUS_SIZE];
    // Registers start at offset 72 in prstatus
    let reg_offset = 72;
    for (i, &val) in registers.iter().enumerate() {
        let off = reg_offset + i * 4;
        prstatus[off..off + 4].copy_from_slice(&val.to_le_bytes());
    }
    output.write_all(&prstatus)?;

    // Pad note to 4-byte alignment
    let pad_bytes = (align4(note_descsz) - note_descsz) as usize;
    if pad_bytes > 0 {
        output.write_all(&vec![0u8; pad_bytes])?;
    }

    // Write memory region data
    for (_base_addr, region_data) in memory_regions {
        output.write_all(region_data)?;
    }

    output.flush()?;
    Ok(())
}

/// Write a raw core dump: JSON manifest + binary region files.
pub fn write_raw_coredump(
    output_path: &str,
    registers: &[(String, u32)],
    memory_regions: &[(String, u64, Vec<u8>)],
) -> std::io::Result<()> {
    // Write JSON manifest
    let manifest = serde_json::json!({
        "format": "raw_coredump",
        "registers": registers.iter().map(|(name, val)| {
            serde_json::json!({ "name": name, "value": format!("0x{:08X}", val) })
        }).collect::<Vec<_>>(),
        "regions": memory_regions.iter().map(|(name, base, data)| {
            serde_json::json!({
                "name": name,
                "base_address": format!("0x{:08X}", base),
                "size": data.len(),
                "file": format!("{}.{}.bin", output_path, name),
            })
        }).collect::<Vec<_>>(),
    });

    let manifest_path = format!("{}.json", output_path);
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap())?;

    // Write binary region files
    for (name, _base, data) in memory_regions {
        let region_path = format!("{}.{}.bin", output_path, name);
        std::fs::write(&region_path, data)?;
    }

    Ok(())
}

fn align4(val: u32) -> u32 {
    (val + 3) & !3
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_registers() -> [u32; NUM_REGS] {
        [
            0xDEADBEEF, // R0
            0x00000001, // R1
            0x00000002, // R2
            0x00000003, // R3
            0x00000004, // R4
            0x00000005, // R5
            0x00000006, // R6
            0x00000007, // R7
            0x00000008, // R8
            0x00000009, // R9
            0x0000000A, // R10
            0x0000000B, // R11
            0x0000000C, // R12
            0x20004000, // SP
            0x08001001, // LR
            0x08000100, // PC
            0x61000000, // xPSR
        ]
    }

    #[test]
    fn test_elf_header_magic() {
        let regs = sample_registers();
        let mut buf = Vec::new();
        write_elf_coredump(&mut buf, &regs, &[]).unwrap();
        assert_eq!(&buf[0..4], &ELFMAG);
    }

    #[test]
    fn test_elf_header_fields() {
        let regs = sample_registers();
        let mut buf = Vec::new();
        write_elf_coredump(&mut buf, &regs, &[]).unwrap();

        // EI_CLASS = ELFCLASS32
        assert_eq!(buf[4], ELFCLASS32);
        // EI_DATA = ELFDATA2LSB
        assert_eq!(buf[5], ELFDATA2LSB);
        // e_type = ET_CORE (4) at offset 16
        assert_eq!(u16::from_le_bytes([buf[16], buf[17]]), ET_CORE);
        // e_machine = EM_ARM (40) at offset 18
        assert_eq!(u16::from_le_bytes([buf[18], buf[19]]), EM_ARM);
    }

    #[test]
    fn test_note_segment_present() {
        let regs = sample_registers();
        let mut buf = Vec::new();
        write_elf_coredump(&mut buf, &regs, &[]).unwrap();

        // e_phoff at offset 28
        let phoff = u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]) as usize;
        // First phdr: p_type at phoff
        let p_type = u32::from_le_bytes([buf[phoff], buf[phoff + 1], buf[phoff + 2], buf[phoff + 3]]);
        assert_eq!(p_type, PT_NOTE);
    }

    #[test]
    fn test_load_segments_match_regions() {
        let regs = sample_registers();
        let region1_data = vec![0xAA; 256];
        let region2_data = vec![0xBB; 512];
        let regions: Vec<(u64, &[u8])> = vec![
            (0x20000000, &region1_data),
            (0x20010000, &region2_data),
        ];

        let mut buf = Vec::new();
        write_elf_coredump(&mut buf, &regs, &regions).unwrap();

        // e_phnum at offset 44
        let phnum = u16::from_le_bytes([buf[44], buf[45]]) as usize;
        assert_eq!(phnum, 3); // 1 PT_NOTE + 2 PT_LOAD

        let phoff = u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]) as usize;

        // Second phdr (index 1) = first PT_LOAD
        let phdr1 = phoff + ELF32_PHDR_SIZE as usize;
        let p_type1 = u32::from_le_bytes([buf[phdr1], buf[phdr1 + 1], buf[phdr1 + 2], buf[phdr1 + 3]]);
        assert_eq!(p_type1, PT_LOAD);
        let p_vaddr1 = u32::from_le_bytes([buf[phdr1 + 8], buf[phdr1 + 9], buf[phdr1 + 10], buf[phdr1 + 11]]);
        assert_eq!(p_vaddr1, 0x20000000);
        let p_filesz1 = u32::from_le_bytes([buf[phdr1 + 16], buf[phdr1 + 17], buf[phdr1 + 18], buf[phdr1 + 19]]);
        assert_eq!(p_filesz1, 256);

        // Third phdr (index 2) = second PT_LOAD
        let phdr2 = phoff + 2 * ELF32_PHDR_SIZE as usize;
        let p_vaddr2 = u32::from_le_bytes([buf[phdr2 + 8], buf[phdr2 + 9], buf[phdr2 + 10], buf[phdr2 + 11]]);
        assert_eq!(p_vaddr2, 0x20010000);
        let p_filesz2 = u32::from_le_bytes([buf[phdr2 + 16], buf[phdr2 + 17], buf[phdr2 + 18], buf[phdr2 + 19]]);
        assert_eq!(p_filesz2, 512);
    }

    #[test]
    fn test_register_values_roundtrip() {
        let regs = sample_registers();
        let mut buf = Vec::new();
        write_elf_coredump(&mut buf, &regs, &[]).unwrap();

        // Find the note segment data
        let phoff = u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]) as usize;
        // PT_NOTE p_offset at phoff+4
        let note_off = u32::from_le_bytes([buf[phoff + 4], buf[phoff + 5], buf[phoff + 6], buf[phoff + 7]]) as usize;

        // Note header: namesz(4) + descsz(4) + type(4) + name(8) = 20 bytes
        // Then prstatus descriptor starts; registers at offset 72 within prstatus
        let desc_start = note_off + 20;
        let reg_start = desc_start + 72;

        // Read back R0
        let r0 = u32::from_le_bytes([buf[reg_start], buf[reg_start + 1], buf[reg_start + 2], buf[reg_start + 3]]);
        assert_eq!(r0, 0xDEADBEEF);

        // Read back SP (index 13)
        let sp_off = reg_start + 13 * 4;
        let sp = u32::from_le_bytes([buf[sp_off], buf[sp_off + 1], buf[sp_off + 2], buf[sp_off + 3]]);
        assert_eq!(sp, 0x20004000);

        // Read back PC (index 15)
        let pc_off = reg_start + 15 * 4;
        let pc = u32::from_le_bytes([buf[pc_off], buf[pc_off + 1], buf[pc_off + 2], buf[pc_off + 3]]);
        assert_eq!(pc, 0x08000100);
    }

    #[test]
    fn test_empty_memory_regions() {
        let regs = sample_registers();
        let mut buf = Vec::new();
        write_elf_coredump(&mut buf, &regs, &[]).unwrap();

        // Should still be valid: 1 PT_NOTE phdr only
        let phnum = u16::from_le_bytes([buf[44], buf[45]]);
        assert_eq!(phnum, 1);
        // Magic is still valid
        assert_eq!(&buf[0..4], &ELFMAG);
    }
}
