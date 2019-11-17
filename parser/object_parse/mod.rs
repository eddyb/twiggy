use std::borrow::{Borrow, Cow};

use gimli;
use object::{self, Object};
use twiggy_ir as ir;
use twiggy_traits as traits;
use typed_arena::Arena;

mod compilation_unit_parse;
mod die_parse;

use self::compilation_unit_parse::CompUnitParser;
use super::Parse;

// Helper function used to load a given section of the file.
fn load_section<'a, 'file, 'input, Sect, Endian>(
    arena: &'a Arena<Cow<'file, [u8]>>,
    file: &'file object::File<'input>,
    endian: Endian,
) -> Sect
where
    Sect: gimli::Section<gimli::EndianSlice<'a, Endian>>,
    Endian: gimli::Endianity,
    'file: 'input,
    'a: 'file,
{
    let data = file
        .section_data_by_name(Sect::section_name())
        .unwrap_or(Cow::Borrowed(&[]));
    let data_ref = (*arena.alloc(data)).borrow();
    Sect::from(gimli::EndianSlice::new(data_ref, endian))
}

pub fn parse(items: &mut ir::ItemsBuilder, data: &[u8]) -> Result<(), traits::Error> {
    let file: object::File = object::File::parse(data)?;

    // Identify the file's endianty and create a typed arena to load sections.
    let arena = Arena::new();
    let endian = if file.is_little_endian() {
        gimli::RunTimeEndian::Little
    } else {
        gimli::RunTimeEndian::Big
    };

    // Load the sections of the file containing debugging information.
    let debug_abbrev: gimli::DebugAbbrev<_> = load_section(&arena, &file, endian);
    let debug_addr: gimli::DebugAddr<_> = load_section(&arena, &file, endian);
    let debug_info: gimli::DebugInfo<_> = load_section(&arena, &file, endian);
    let debug_line: gimli::DebugLine<_> = load_section(&arena, &file, endian);
    let debug_line_str: gimli::DebugLineStr<_> = load_section(&arena, &file, endian);
    let debug_str: gimli::DebugStr<_> = load_section(&arena, &file, endian);
    let debug_str_offsets: gimli::DebugStrOffsets<_> = load_section(&arena, &file, endian);
    let debug_ranges: gimli::DebugRanges<_> = load_section(&arena, &file, endian);
    let debug_rnglists: gimli::DebugRngLists<_> = load_section(&arena, &file, endian);
    let ranges = gimli::RangeLists::new(debug_ranges, debug_rnglists);
    let mut dwarf = gimli::Dwarf {
        debug_abbrev,
        debug_addr,
        debug_info,
        debug_line,
        debug_line_str,
        debug_str,
        debug_str_offsets,
        ranges,
        ..Default::default()
    };

    dwarf.parse_items(items, ())?;
    dwarf.parse_edges(items, ())?;
    Ok(())
}

impl<'a, R: gimli::Reader<Offset = usize>> Parse<'a> for gimli::Dwarf<R> {
    type ItemsExtra = ();

    fn parse_items(
        &mut self,
        items: &mut ir::ItemsBuilder,
        _extra: Self::ItemsExtra,
    ) -> Result<(), traits::Error> {
        let mut parser = CompUnitParser::default();

        // Parse the items in each compilation unit.
        let mut headers = self.units();
        while let Some(header) = headers.next()? {
            parser.parse(self, header)?;
        }

        for (offset, subroutine) in parser.subroutines {
            let name = subroutine
                .name
                .unwrap_or_else(|| format!("Subroutine[{:#x}]", offset.0));
            let kind: ir::ItemKind = ir::Code::new(&name).into();
            let id = ir::Id::entry(0, offset.0);
            items.add_item(ir::Item::new(id, name, subroutine.size, kind));
        }

        Ok(())
    }

    type EdgesExtra = ();

    fn parse_edges(
        &mut self,
        _items: &mut ir::ItemsBuilder,
        _extra: Self::EdgesExtra,
    ) -> Result<(), traits::Error> {
        Ok(())
    }
}
