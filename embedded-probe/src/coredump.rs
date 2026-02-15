//! Core dump generation and analysis for ARM Cortex-M targets.
//!
//! Two capabilities:
//! 1. **ELF core dump generation** — produces GDB-compatible ELF core files
//! 2. **Zephyr coredump parsing** — parses `#CD:` prefixed hex lines from Zephyr's
//!    logging-based coredump backend and produces crash analysis reports

use std::io::Write;
use crate::symbols::SymbolTable;

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

// =============================================================================
// Zephyr Coredump Parser (#CD: log format)
// =============================================================================

/// Zephyr coredump target codes
const COREDUMP_TGT_ARM_CORTEX_M: u16 = 3;

/// Zephyr coredump reason codes
fn reason_string(code: u32) -> &'static str {
    match code {
        0 => "K_ERR_CPU_EXCEPTION",
        1 => "K_ERR_SPURIOUS_IRQ",
        2 => "K_ERR_STACK_CHK_FAIL",
        3 => "K_ERR_KERNEL_OOPS",
        4 => "K_ERR_KERNEL_PANIC",
        _ => "Unknown",
    }
}

/// ARM Cortex-M registers extracted from a Zephyr coredump.
///
/// Register order in the coredump (v1: 9 regs, v2: 17 regs):
///   R0, R1, R2, R3, R12, LR, PC, xPSR, SP, [R4-R11 in v2]
#[derive(Debug, Default)]
pub struct ZephyrCortexMRegs {
    pub r0: u32,
    pub r1: u32,
    pub r2: u32,
    pub r3: u32,
    pub r4: u32,
    pub r5: u32,
    pub r6: u32,
    pub r7: u32,
    pub r8: u32,
    pub r9: u32,
    pub r10: u32,
    pub r11: u32,
    pub r12: u32,
    pub sp: u32,
    pub lr: u32,
    pub pc: u32,
    pub xpsr: u32,
}

/// Parsed Zephyr coredump data.
#[derive(Debug)]
pub struct ZephyrCoredump {
    pub reason: &'static str,
    pub registers: ZephyrCortexMRegs,
    pub memory_regions: Vec<(u64, Vec<u8>)>,
}

/// Extract `#CD:` prefixed hex data from log text and concatenate into raw bytes.
fn extract_coredump_bytes(log_text: &str) -> Result<Vec<u8>, String> {
    let mut hex_data = String::new();
    let mut in_coredump = false;

    for line in log_text.lines() {
        // Find #CD: prefix (may have log prefixes before it)
        if let Some(pos) = line.find("#CD:") {
            let after_prefix = &line[pos + 4..];
            let trimmed = after_prefix.trim();
            if trimmed == "BEGIN#" {
                in_coredump = true;
                continue;
            }
            if trimmed == "END#" {
                break;
            }
            if in_coredump {
                hex_data.push_str(trimmed);
            }
        }
    }

    if hex_data.is_empty() {
        return Err("No #CD: coredump data found in log text".to_string());
    }

    hex::decode(&hex_data).map_err(|e| format!("Failed to decode coredump hex data: {}", e))
}

/// Read a little-endian u16 from a byte slice at the given offset.
fn read_u16_le(data: &[u8], offset: usize) -> Result<u16, String> {
    if offset + 2 > data.len() {
        return Err(format!("Unexpected end of data at offset {}", offset));
    }
    Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

/// Read a little-endian u32 from a byte slice at the given offset.
fn read_u32_le(data: &[u8], offset: usize) -> Result<u32, String> {
    if offset + 4 > data.len() {
        return Err(format!("Unexpected end of data at offset {}", offset));
    }
    Ok(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

/// Parse `#CD:` prefixed lines from log text into a `ZephyrCoredump`.
pub fn parse_zephyr_coredump(log_text: &str) -> Result<ZephyrCoredump, String> {
    let data = extract_coredump_bytes(log_text)?;

    // --- File header (12 bytes) ---
    if data.len() < 12 {
        return Err("Coredump data too short for file header".to_string());
    }
    if data[0] != b'Z' || data[1] != b'E' {
        return Err(format!(
            "Invalid coredump magic: expected 'ZE', got '{}{}'",
            data[0] as char, data[1] as char
        ));
    }
    let _hdr_version = read_u16_le(&data, 2)?;
    let tgt_code = read_u16_le(&data, 4)?;
    if tgt_code != COREDUMP_TGT_ARM_CORTEX_M {
        return Err(format!(
            "Unsupported target code: {} (only ARM Cortex-M = {} supported)",
            tgt_code, COREDUMP_TGT_ARM_CORTEX_M
        ));
    }
    let ptr_size_bits = data[6];
    let ptr_size = if ptr_size_bits == 5 { 4usize } else if ptr_size_bits == 6 { 8 } else {
        return Err(format!("Unsupported pointer size bits: {}", ptr_size_bits));
    };
    let reason_code = read_u32_le(&data, 8)?;
    let reason = reason_string(reason_code);

    let mut offset = 12;
    let mut registers = ZephyrCortexMRegs::default();
    let mut memory_regions = Vec::new();

    // Parse blocks until end of data
    while offset < data.len() {
        if data.len() - offset < 1 {
            break;
        }
        let block_id = data[offset] as char;

        match block_id {
            // Architecture block: 'A' + version(u16) + num_bytes(u16) + register data
            'A' => {
                if offset + 5 > data.len() {
                    return Err("Truncated architecture block header".to_string());
                }
                let arch_version = read_u16_le(&data, offset + 1)?;
                let num_bytes = read_u16_le(&data, offset + 3)? as usize;
                offset += 5; // skip header

                if offset + num_bytes > data.len() {
                    return Err("Truncated architecture block data".to_string());
                }

                // Parse registers based on version
                let reg_data = &data[offset..offset + num_bytes];
                let num_regs = num_bytes / 4;

                // v1 order (9 regs): R0,R1,R2,R3,R12,LR,PC,xPSR,SP
                // v2 order (17 regs): R0,R1,R2,R3,R12,LR,PC,xPSR,SP,R4,R5,R6,R7,R8,R9,R10,R11
                if num_regs >= 9 {
                    registers.r0 = read_u32_le(reg_data, 0)?;
                    registers.r1 = read_u32_le(reg_data, 4)?;
                    registers.r2 = read_u32_le(reg_data, 8)?;
                    registers.r3 = read_u32_le(reg_data, 12)?;
                    registers.r12 = read_u32_le(reg_data, 16)?;
                    registers.lr = read_u32_le(reg_data, 20)?;
                    registers.pc = read_u32_le(reg_data, 24)?;
                    registers.xpsr = read_u32_le(reg_data, 28)?;
                    registers.sp = read_u32_le(reg_data, 32)?;
                }
                if num_regs >= 17 && arch_version >= 2 {
                    registers.r4 = read_u32_le(reg_data, 36)?;
                    registers.r5 = read_u32_le(reg_data, 40)?;
                    registers.r6 = read_u32_le(reg_data, 44)?;
                    registers.r7 = read_u32_le(reg_data, 48)?;
                    registers.r8 = read_u32_le(reg_data, 52)?;
                    registers.r9 = read_u32_le(reg_data, 56)?;
                    registers.r10 = read_u32_le(reg_data, 60)?;
                    registers.r11 = read_u32_le(reg_data, 64)?;
                }

                offset += num_bytes;
            }

            // Memory block: 'M' + version(u16) + start(ptr) + end(ptr) + data
            'M' => {
                if offset + 3 > data.len() {
                    return Err("Truncated memory block header".to_string());
                }
                let _mem_version = read_u16_le(&data, offset + 1)?;
                offset += 3; // skip id + version

                if ptr_size == 4 {
                    if offset + 8 > data.len() {
                        return Err("Truncated memory block addresses".to_string());
                    }
                    let start = read_u32_le(&data, offset)? as u64;
                    let end = read_u32_le(&data, offset + 4)? as u64;
                    offset += 8;

                    let size = (end - start) as usize;
                    if offset + size > data.len() {
                        return Err(format!(
                            "Truncated memory block data: need {} bytes at offset {}, have {}",
                            size, offset, data.len() - offset
                        ));
                    }
                    memory_regions.push((start, data[offset..offset + size].to_vec()));
                    offset += size;
                } else {
                    // 64-bit pointers
                    if offset + 16 > data.len() {
                        return Err("Truncated memory block addresses (64-bit)".to_string());
                    }
                    let start_lo = read_u32_le(&data, offset)? as u64;
                    let start_hi = read_u32_le(&data, offset + 4)? as u64;
                    let end_lo = read_u32_le(&data, offset + 8)? as u64;
                    let end_hi = read_u32_le(&data, offset + 12)? as u64;
                    let start = start_lo | (start_hi << 32);
                    let end = end_lo | (end_hi << 32);
                    offset += 16;

                    let size = (end - start) as usize;
                    if offset + size > data.len() {
                        return Err("Truncated memory block data (64-bit)".to_string());
                    }
                    memory_regions.push((start, data[offset..offset + size].to_vec()));
                    offset += size;
                }
            }

            // Threads metadata block: 'T' + version(u16) + num_bytes(u16) + data (skip)
            'T' => {
                if offset + 5 > data.len() {
                    return Err("Truncated threads metadata header".to_string());
                }
                let num_bytes = read_u16_le(&data, offset + 3)? as usize;
                offset += 5 + num_bytes;
            }

            _ => {
                // Unknown block — can't determine size, stop parsing
                break;
            }
        }
    }

    Ok(ZephyrCoredump {
        reason,
        registers,
        memory_regions,
    })
}

/// Format a crash analysis report with symbol resolution from an ELF file.
pub fn format_crash_report(dump: &ZephyrCoredump, symbols: &SymbolTable) -> String {
    let regs = &dump.registers;
    let mut report = String::new();

    report.push_str("=== Crash Analysis Report ===\n\n");
    report.push_str(&format!("Fault reason: {}\n", dump.reason));

    // Crash PC with symbol
    let pc_sym = symbols.resolve(regs.pc as u64);
    report.push_str(&format!("Crash PC:     0x{:08X}", regs.pc));
    if let Some(ref sym) = pc_sym {
        report.push_str(&format!(" → {}", sym));
    }
    report.push('\n');

    // Caller (LR) with symbol
    let lr_sym = symbols.resolve(regs.lr as u64);
    report.push_str(&format!("Caller (LR):  0x{:08X}", regs.lr));
    if let Some(ref sym) = lr_sym {
        report.push_str(&format!(" → {}", sym));
    }
    report.push('\n');

    report.push_str(&format!("Stack (SP):   0x{:08X}\n", regs.sp));

    // Register dump
    report.push_str("\nRegister Dump:\n");
    report.push_str(&format!(
        "  R0:  0x{:08X}   R1:  0x{:08X}   R2:  0x{:08X}   R3:  0x{:08X}\n",
        regs.r0, regs.r1, regs.r2, regs.r3
    ));
    report.push_str(&format!(
        "  R4:  0x{:08X}   R5:  0x{:08X}   R6:  0x{:08X}   R7:  0x{:08X}\n",
        regs.r4, regs.r5, regs.r6, regs.r7
    ));
    report.push_str(&format!(
        "  R8:  0x{:08X}   R9:  0x{:08X}   R10: 0x{:08X}   R11: 0x{:08X}\n",
        regs.r8, regs.r9, regs.r10, regs.r11
    ));
    report.push_str(&format!(
        "  R12: 0x{:08X}   SP:  0x{:08X}   LR:  0x{:08X}   PC:  0x{:08X}\n",
        regs.r12, regs.sp, regs.lr, regs.pc
    ));
    report.push_str(&format!("  xPSR: 0x{:08X}\n", regs.xpsr));

    // Call chain from LR/stack
    report.push_str("\nCall chain (from exception frame):\n");
    report.push_str(&format!("  #0 0x{:08X} ", regs.pc));
    if let Some(ref sym) = pc_sym {
        report.push_str(&format!("{:<40}", format!("{}", sym)));
    } else {
        report.push_str(&format!("{:<40}", "???"));
    }
    report.push_str(" ← CRASH HERE\n");

    report.push_str(&format!("  #1 0x{:08X} ", regs.lr));
    if let Some(ref sym) = lr_sym {
        report.push_str(&format!("{}", sym));
    } else {
        report.push_str("???");
    }
    report.push('\n');

    // Try to walk stack for more frames if we have memory data covering SP
    let mut frame_idx = 2;
    if let Some(region) = dump.memory_regions.iter().find(|(base, data)| {
        let end = *base + data.len() as u64;
        (regs.sp as u64) >= *base && (regs.sp as u64) < end
    }) {
        let (base, data) = region;
        let sp_offset = (regs.sp as u64 - base) as usize;
        // Scan stack words for potential return addresses
        let mut pos = sp_offset;
        while pos + 4 <= data.len() && frame_idx < 10 {
            let word = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            // Check if this looks like a Thumb return address (odd, in code range)
            if word & 1 == 1 && word >= 0x00000100 && word < 0x20000000 {
                if let Some(sym) = symbols.resolve(word as u64) {
                    report.push_str(&format!("  #{} 0x{:08X} {}\n", frame_idx, word, sym));
                    frame_idx += 1;
                }
            }
            pos += 4;
        }
    }

    // Memory region summary
    if !dump.memory_regions.is_empty() {
        report.push_str(&format!(
            "\nMemory regions captured: {} (",
            dump.memory_regions.len()
        ));
        let parts: Vec<String> = dump.memory_regions.iter().map(|(base, data)| {
            let size = data.len();
            if size >= 1024 {
                format!("{} KB from 0x{:08X}", size / 1024, base)
            } else {
                format!("{} bytes from 0x{:08X}", size, base)
            }
        }).collect();
        report.push_str(&parts.join(", "));
        report.push_str(")\n");
    }

    report
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

    // =========================================================================
    // Zephyr Coredump Parser Tests
    // =========================================================================

    /// Build a synthetic Zephyr coredump binary and encode as #CD: log lines.
    fn build_coredump_log(
        reason: u32,
        regs_v2: &[u32; 17], // R0,R1,R2,R3,R12,LR,PC,xPSR,SP,R4-R11
        memory: Option<(u32, &[u8])>,
    ) -> String {
        let mut bin = Vec::new();

        // File header: 'Z','E', version=2, tgt=ARM_CORTEX_M(3), ptr_size_bits=5(32-bit), flag=0, reason
        bin.extend_from_slice(&[b'Z', b'E']);
        bin.extend_from_slice(&2u16.to_le_bytes()); // hdr_version
        bin.extend_from_slice(&3u16.to_le_bytes()); // tgt_code = ARM_CORTEX_M
        bin.push(5); // ptr_size_bits = 5 (32-bit)
        bin.push(0); // flag
        bin.extend_from_slice(&reason.to_le_bytes()); // reason

        // Architecture block: 'A', version=2, num_bytes=68 (17 regs * 4)
        bin.push(b'A');
        bin.extend_from_slice(&2u16.to_le_bytes()); // hdr_version
        bin.extend_from_slice(&68u16.to_le_bytes()); // num_bytes
        for reg in regs_v2 {
            bin.extend_from_slice(&reg.to_le_bytes());
        }

        // Memory block if provided
        if let Some((base, data)) = memory {
            let end = base + data.len() as u32;
            bin.push(b'M');
            bin.extend_from_slice(&1u16.to_le_bytes()); // hdr_version
            bin.extend_from_slice(&base.to_le_bytes()); // start
            bin.extend_from_slice(&end.to_le_bytes()); // end
            bin.extend_from_slice(data);
        }

        // Encode as #CD: log lines (64 hex chars = 32 bytes per line)
        let hex_str = hex::encode(&bin);
        let mut log = String::new();
        log.push_str("[00:00:00.000,000] <inf> coredump: #CD:BEGIN#\n");
        for chunk in hex_str.as_bytes().chunks(64) {
            log.push_str("[00:00:00.000,000] <inf> coredump: #CD:");
            log.push_str(std::str::from_utf8(chunk).unwrap());
            log.push('\n');
        }
        log.push_str("[00:00:00.000,000] <inf> coredump: #CD:END#\n");
        log
    }

    #[test]
    fn test_parse_zephyr_coredump_registers() {
        // v2 order: R0,R1,R2,R3,R12,LR,PC,xPSR,SP,R4,R5,R6,R7,R8,R9,R10,R11
        let regs = [
            0x00000000, // R0
            0x0002B100, // R1
            0x0000BEEF, // R2
            0x00000003, // R3
            0x0000000C, // R12
            0x0002A3E1, // LR
            0x0002A3F4, // PC
            0x61000000, // xPSR
            0x2000FE80, // SP
            0x20001234, // R4
            0x00000005, // R5
            0x00000006, // R6
            0x2000FEA0, // R7
            0x00000008, // R8
            0x00000009, // R9
            0x0000000A, // R10
            0x0000000B, // R11
        ];

        let log = build_coredump_log(0, &regs, None);
        let dump = parse_zephyr_coredump(&log).unwrap();

        assert_eq!(dump.reason, "K_ERR_CPU_EXCEPTION");
        assert_eq!(dump.registers.pc, 0x0002A3F4);
        assert_eq!(dump.registers.lr, 0x0002A3E1);
        assert_eq!(dump.registers.sp, 0x2000FE80);
        assert_eq!(dump.registers.r0, 0x00000000);
        assert_eq!(dump.registers.r2, 0x0000BEEF);
        assert_eq!(dump.registers.r4, 0x20001234);
        assert_eq!(dump.registers.r7, 0x2000FEA0);
        assert_eq!(dump.registers.xpsr, 0x61000000);
    }

    #[test]
    fn test_parse_zephyr_coredump_with_memory() {
        let regs = [0u32; 17];
        let ram_data = vec![0xAA; 64];
        let log = build_coredump_log(0, &regs, Some((0x20000000, &ram_data)));
        let dump = parse_zephyr_coredump(&log).unwrap();

        assert_eq!(dump.memory_regions.len(), 1);
        assert_eq!(dump.memory_regions[0].0, 0x20000000);
        assert_eq!(dump.memory_regions[0].1.len(), 64);
        assert!(dump.memory_regions[0].1.iter().all(|&b| b == 0xAA));
    }

    #[test]
    fn test_parse_zephyr_coredump_reason_codes() {
        let regs = [0u32; 17];
        for (code, expected) in [
            (0, "K_ERR_CPU_EXCEPTION"),
            (1, "K_ERR_SPURIOUS_IRQ"),
            (2, "K_ERR_STACK_CHK_FAIL"),
            (3, "K_ERR_KERNEL_OOPS"),
            (4, "K_ERR_KERNEL_PANIC"),
            (99, "Unknown"),
        ] {
            let log = build_coredump_log(code, &regs, None);
            let dump = parse_zephyr_coredump(&log).unwrap();
            assert_eq!(dump.reason, expected);
        }
    }

    #[test]
    fn test_parse_zephyr_coredump_no_data() {
        let result = parse_zephyr_coredump("some random log output\nno coredump here");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No #CD: coredump data found"));
    }

    #[test]
    fn test_parse_zephyr_coredump_bad_magic() {
        let log = "#CD:BEGIN#\n#CD:4242020003000500000000000000\n#CD:END#\n";
        let result = parse_zephyr_coredump(log);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid coredump magic"));
    }

    #[test]
    fn test_parse_zephyr_coredump_with_log_prefixes() {
        // Simulate real RTT output with Zephyr log prefixes
        let regs = [0x11111111u32; 17];
        let log = build_coredump_log(0, &regs, None);
        let dump = parse_zephyr_coredump(&log).unwrap();
        assert_eq!(dump.registers.r0, 0x11111111);
    }

    #[test]
    fn test_format_crash_report_with_symbols() {
        let regs = [
            0x00000000, 0x0002B100, 0x0000BEEF, 0x00000003,
            0x0000000C, 0x0002A3E1, 0x0002A3F4, 0x61000000,
            0x2000FE80, 0x20001234, 0, 0, 0x2000FEA0, 0, 0, 0, 0,
        ];
        let log = build_coredump_log(0, &regs, None);
        let dump = parse_zephyr_coredump(&log).unwrap();

        let symbols = SymbolTable::from_entries(vec![
            ("sensor_read_register", 0x0002A3EC, 16),
            ("sensor_process_data", 0x0002A3C4, 40),
            ("sensor_init_sequence", 0x0002A3B4, 16),
            ("main", 0x0002A380, 52),
        ]);

        let report = format_crash_report(&dump, &symbols);
        assert!(report.contains("Crash PC:     0x0002A3F4"));
        assert!(report.contains("sensor_read_register"));
        assert!(report.contains("Caller (LR):  0x0002A3E1"));
        assert!(report.contains("sensor_process_data"));
        assert!(report.contains("CRASH HERE"));
        assert!(report.contains("K_ERR_CPU_EXCEPTION"));
    }

    #[test]
    fn test_format_crash_report_no_symbols() {
        let regs = [0u32; 17];
        let log = build_coredump_log(3, &regs, None);
        let dump = parse_zephyr_coredump(&log).unwrap();

        let symbols = SymbolTable::from_entries(vec![]);
        let report = format_crash_report(&dump, &symbols);
        assert!(report.contains("K_ERR_KERNEL_OOPS"));
        assert!(report.contains("???"));
    }
}
