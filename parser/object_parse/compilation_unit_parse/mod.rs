use gimli;
use std::collections::BTreeMap;
use twiggy_traits as traits;

use super::die_parse::item_name::item_name;
use super::die_parse::location_attrs::DieLocationAttributes;

#[derive(Default)]
pub struct Subroutine {
    pub name: Option<String>,
    pub size: u32,
}

#[derive(Default)]
pub struct CompUnitParser {
    pub subroutines: BTreeMap<gimli::DebugInfoOffset, Subroutine>,
}

enum Node {
    Subprogram(gimli::DebugInfoOffset),
    InlinedSubroutine(gimli::DebugInfoOffset),
    Other,
}

impl CompUnitParser {
    pub fn parse<'input, R>(
        &mut self,
        dwarf: &'input gimli::Dwarf<R>,
        header: gimli::CompilationUnitHeader<R>,
    ) -> Result<(), traits::Error>
    where
        R: 'input + gimli::Reader<Offset = usize>,
    {
        let unit = dwarf.unit(header.clone())?;

        // Create an entries cursor, and move it to the root.
        let mut die_cursor = unit.entries();

        if die_cursor.next_dfs()?.is_none() {
            let e = traits::Error::with_msg(
                "Unexpected error while traversing debugging information entries.",
            );
            return Err(e);
        }

        // Parse the contained debugging information entries in depth-first order.
        let mut stack: Vec<Node> = vec![];
        'dfs: while let Some((delta, entry)) = die_cursor.next_dfs()? {
            // Update the stack, and break out of the loop when we
            // return to the original starting position.
            for _ in delta..=0 {
                if stack.pop().is_none() {
                    assert!(die_cursor.next_dfs()?.is_none());
                    break 'dfs;
                }
            }

            let get_abstract_origin = || -> gimli::Result<_> {
                Ok(entry
                    .attr_value(gimli::DW_AT_abstract_origin)?
                    .map(|origin| match origin {
                        gimli::AttributeValue::UnitRef(offset) => {
                            offset.to_debug_info_offset(&header)
                        }
                        gimli::AttributeValue::DebugInfoRef(offset) => offset,
                        attr => panic!("unexpected `DW_AT_abstract_origin` value `{:?}`", attr),
                    }))
            };

            let node = match entry.tag() {
                gimli::DW_TAG_subprogram => Node::Subprogram(
                    get_abstract_origin()?
                        .unwrap_or_else(|| entry.offset().to_debug_info_offset(&header)),
                ),
                gimli::DW_TAG_inlined_subroutine => {
                    Node::InlinedSubroutine(get_abstract_origin()?.unwrap())
                }
                _ => Node::Other,
            };

            match node {
                Node::Subprogram(offset) | Node::InlinedSubroutine(offset) => {
                    let subroutine = self.subroutines.entry(offset).or_default();

                    if let Node::Subprogram(_) = node {
                        if let Some(name) = item_name(entry, dwarf, &unit)? {
                            assert_eq!(subroutine.name, None);
                            subroutine.name = Some(name);
                        }
                    }

                    let size = DieLocationAttributes::try_from(entry)?
                        .entity_size(dwarf, &unit)?
                        .unwrap_or(0) as u32;
                    subroutine.size += size;

                    if let Node::InlinedSubroutine(_) = node {
                        // Subtract the inlined size from the innermost
                        // sorrounding `Subprogram` or `InlinedSubroutine`.
                        for node in stack.iter_mut().rev() {
                            match node {
                                Node::Subprogram(caller_offset)
                                | Node::InlinedSubroutine(caller_offset) => {
                                    let caller = self.subroutines.get_mut(caller_offset).unwrap();
                                    // FIXME(eddyb) take ranges into account properly, even when they overlap.
                                    if caller.size >= size {
                                        caller.size -= size;
                                    }
                                    break;
                                }
                                Node::Other => {}
                            }
                        }
                    }
                }
                Node::Other => {}
            }

            stack.push(node);
        }

        Ok(())
    }
}
