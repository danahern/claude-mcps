use std::path::Path;

/// A resolved symbol with name, base address, and offset from start.
#[derive(Debug, Clone)]
pub struct ResolvedSymbol {
    pub name: String,
    pub address: u64,
    pub offset: u64,
}

/// Sorted table of function symbols from an ELF file.
pub struct SymbolTable {
    symbols: Vec<Symbol>,
}

#[derive(Debug, Clone)]
struct Symbol {
    name: String,
    address: u64,
    size: u64,
}

/// Maximum offset heuristic for symbols with size=0.
const MAX_ZERO_SIZE_OFFSET: u64 = 4096;

impl SymbolTable {
    /// Parse an ELF file and collect function symbols, sorted by address.
    pub fn from_elf(path: &Path) -> anyhow::Result<Self> {
        let data = std::fs::read(path)?;
        Self::from_elf_bytes(&data)
    }

    /// Parse ELF bytes and collect function symbols, sorted by address.
    pub fn from_elf_bytes(data: &[u8]) -> anyhow::Result<Self> {
        let elf = goblin::elf::Elf::parse(data)?;
        let mut symbols = Vec::new();

        for sym in &elf.syms {
            // Only collect function symbols with nonzero address
            if sym.is_function() && sym.st_value != 0 {
                if let Some(name) = elf.strtab.get_at(sym.st_name) {
                    if !name.is_empty() {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            // Clear Thumb bit (bit 0) for ARM
                            address: sym.st_value & !1,
                            size: sym.st_size,
                        });
                    }
                }
            }
        }

        symbols.sort_by_key(|s| s.address);
        // Deduplicate by address (keep first occurrence)
        symbols.dedup_by_key(|s| s.address);

        Ok(Self { symbols })
    }

    /// Build a SymbolTable directly from (name, address, size) tuples.
    /// Useful for testing.
    pub fn from_entries(entries: Vec<(&str, u64, u64)>) -> Self {
        let mut symbols: Vec<Symbol> = entries
            .into_iter()
            .map(|(name, addr, size)| Symbol {
                name: name.to_string(),
                address: addr & !1, // clear Thumb bit
                size,
            })
            .collect();
        symbols.sort_by_key(|s| s.address);
        Self { symbols }
    }

    /// Resolve an address to the containing function symbol.
    /// Clears Thumb bit before lookup.
    pub fn resolve(&self, addr: u64) -> Option<ResolvedSymbol> {
        if self.symbols.is_empty() {
            return None;
        }

        let addr = addr & !1; // clear Thumb bit

        // Binary search: find the last symbol with address <= addr
        let idx = match self.symbols.binary_search_by_key(&addr, |s| s.address) {
            Ok(i) => i,
            Err(0) => return None, // addr is before all symbols
            Err(i) => i - 1,
        };

        let sym = &self.symbols[idx];
        let offset = addr - sym.address;

        // Check if addr is within the symbol's bounds
        if sym.size > 0 {
            if offset < sym.size {
                Some(ResolvedSymbol {
                    name: sym.name.clone(),
                    address: sym.address,
                    offset,
                })
            } else {
                None
            }
        } else {
            // Zero-size symbol: use heuristic max offset
            if offset <= MAX_ZERO_SIZE_OFFSET {
                Some(ResolvedSymbol {
                    name: sym.name.clone(),
                    address: sym.address,
                    offset,
                })
            } else {
                None
            }
        }
    }

    pub fn symbol_count(&self) -> usize {
        self.symbols.len()
    }
}

impl std::fmt::Display for ResolvedSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.offset == 0 {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}+0x{:x}", self.name, self.offset)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_table(entries: Vec<(&str, u64, u64)>) -> SymbolTable {
        SymbolTable::from_entries(entries)
    }

    #[test]
    fn test_resolve_exact_address() {
        let table = make_table(vec![("main", 0x08000100, 64)]);
        let resolved = table.resolve(0x08000100).unwrap();
        assert_eq!(resolved.name, "main");
        assert_eq!(resolved.offset, 0);
    }

    #[test]
    fn test_resolve_with_offset() {
        let table = make_table(vec![("main", 0x08000100, 64)]);
        let resolved = table.resolve(0x08000110).unwrap();
        assert_eq!(resolved.name, "main");
        assert_eq!(resolved.offset, 0x10);
    }

    #[test]
    fn test_resolve_address_before_first() {
        let table = make_table(vec![("main", 0x08000100, 64)]);
        assert!(table.resolve(0x08000050).is_none());
    }

    #[test]
    fn test_resolve_address_after_last() {
        let table = make_table(vec![("main", 0x08000100, 64)]);
        // 0x08000100 + 64 = 0x08000140, so 0x08000140 is outside
        assert!(table.resolve(0x08000140).is_none());
    }

    #[test]
    fn test_resolve_between_symbols() {
        let table = make_table(vec![
            ("func_a", 0x08000100, 32),
            ("func_b", 0x08000200, 32),
        ]);
        // Address in the gap between func_a (ends at 0x120) and func_b (starts at 0x200)
        assert!(table.resolve(0x08000150).is_none());
    }

    #[test]
    fn test_resolve_zero_size_symbol() {
        let table = make_table(vec![("handler", 0x08000100, 0)]);
        let resolved = table.resolve(0x08000200).unwrap();
        assert_eq!(resolved.name, "handler");
        assert_eq!(resolved.offset, 0x100);
    }

    #[test]
    fn test_resolve_zero_size_too_far() {
        let table = make_table(vec![("handler", 0x08000100, 0)]);
        // 4098 bytes offset (after Thumb bit clearing), over the 4096 heuristic
        // 0x08000100 + 4098 = 0x08001102, Thumb cleared → 0x08001102, offset = 0x1002 = 4098
        assert!(table.resolve(0x08000100 + 4098).is_none());
    }

    #[test]
    fn test_thumb_bit_cleared() {
        let table = make_table(vec![("main", 0x08000100, 64)]);
        // Address with Thumb bit set (odd)
        let resolved = table.resolve(0x08000111).unwrap();
        assert_eq!(resolved.name, "main");
        assert_eq!(resolved.offset, 0x10); // 0x08000110 - 0x08000100
    }

    #[test]
    fn test_empty_table() {
        let table = make_table(vec![]);
        assert!(table.resolve(0x08000100).is_none());
    }

    #[test]
    fn test_single_symbol() {
        let table = make_table(vec![("only_func", 0x08000000, 100)]);
        let resolved = table.resolve(0x08000032).unwrap();
        assert_eq!(resolved.name, "only_func");
        assert_eq!(resolved.offset, 0x32);
    }

    #[test]
    fn test_adjacent_symbols() {
        let table = make_table(vec![
            ("func_a", 0x08000100, 32),
            ("func_b", 0x08000120, 32),
        ]);
        // Last byte of func_a
        let resolved = table.resolve(0x0800011F).unwrap();
        assert_eq!(resolved.name, "func_a");
        // First byte of func_b
        let resolved = table.resolve(0x08000120).unwrap();
        assert_eq!(resolved.name, "func_b");
        assert_eq!(resolved.offset, 0);
    }

    #[test]
    fn test_display_no_offset() {
        let sym = ResolvedSymbol {
            name: "main".to_string(),
            address: 0x08000100,
            offset: 0,
        };
        assert_eq!(format!("{}", sym), "main");
    }

    #[test]
    fn test_display_with_offset() {
        let sym = ResolvedSymbol {
            name: "main".to_string(),
            address: 0x08000100,
            offset: 0x12,
        };
        assert_eq!(format!("{}", sym), "main+0x12");
    }

    #[test]
    fn test_from_elf_nonexistent_file() {
        let result = SymbolTable::from_elf(Path::new("/nonexistent/file.elf"));
        assert!(result.is_err());
    }

    #[test]
    fn test_from_elf_invalid_file() {
        // A non-ELF file
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"not an elf file").unwrap();
        let result = SymbolTable::from_elf(tmp.path());
        assert!(result.is_err());
    }
}
