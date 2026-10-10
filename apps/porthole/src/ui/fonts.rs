//! Retain the original CSS `tabular-nums` using the installed font's own glyphs.
//! The renderer does not expose OpenType feature selection, so apply `tnum` to
//! digit mappings in memory. Installed font files are never changed or bundled.
use std::collections::BTreeMap;
use ttf_parser::{
    Face, GlyphId, Tag,
    gsub::{SingleSubstitution, SubstitutionSubtable},
};

pub fn tabular_digits(bytes: &mut Vec<u8>, index: u32) {
    let Some(cmap) = tabular_cmap(bytes, index) else {
        return;
    };
    let count = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    let Some(record) = (0..count)
        .map(|i| 12 + 16 * i)
        .find(|&at| &bytes[at..at + 4] == b"cmap")
    else {
        return;
    };
    let offset = bytes.len().next_multiple_of(4);
    bytes.resize(offset, 0);
    bytes.extend_from_slice(&cmap);
    bytes.resize(bytes.len().next_multiple_of(4), 0);
    bytes[record + 4..record + 8].copy_from_slice(&checksum(&cmap).to_be_bytes());
    bytes[record + 8..record + 12].copy_from_slice(&(offset as u32).to_be_bytes());
    bytes[record + 12..record + 16].copy_from_slice(&(cmap.len() as u32).to_be_bytes());
    if let Some(head) = (0..count)
        .map(|i| 12 + 16 * i)
        .find(|&at| &bytes[at..at + 4] == b"head")
    {
        let offset = u32::from_be_bytes(bytes[head + 8..head + 12].try_into().unwrap()) as usize + 8;
        bytes[offset..offset + 4].fill(0);
        let adjustment = 0xb1b0_afba_u32.wrapping_sub(checksum(bytes));
        bytes[offset..offset + 4].copy_from_slice(&adjustment.to_be_bytes());
    }
}
fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0, |sum, word| {
        let mut padded = [0; 4];
        padded[..word.len()].copy_from_slice(word);
        sum.wrapping_add(u32::from_be_bytes(padded))
    })
}
fn tabular_cmap(bytes: &[u8], index: u32) -> Option<Vec<u8>> {
    // Collections have shared table directories. Keep their original mappings.
    if bytes.get(..4)? == b"ttcf" {
        return None;
    }
    let face = Face::parse(bytes, index).ok()?;
    let gsub = face.tables().gsub?;
    let feature = gsub.features.find(Tag::from_bytes(b"tnum"))?;
    let mut digits = BTreeMap::new();
    for codepoint in b'0'..=b'9' {
        let mut glyph = face.glyph_index(codepoint as char)?;
        for lookup in feature.lookup_indices {
            for subtable in gsub.lookups.get(lookup)?.subtables.into_iter::<SubstitutionSubtable>() {
                if let SubstitutionSubtable::Single(single) = subtable {
                    match single {
                        SingleSubstitution::Format1 { coverage, delta } if coverage.contains(glyph) => {
                            glyph = GlyphId(glyph.0.wrapping_add_signed(delta));
                        }
                        SingleSubstitution::Format2 { coverage, substitutes } => {
                            if let Some(i) = coverage.get(glyph) {
                                glyph = substitutes.get(i)?;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        digits.insert(codepoint as u32, glyph.0 as u32);
    }
    let mut mappings = BTreeMap::new();
    for subtable in face.tables().cmap?.subtables {
        if subtable.is_unicode() {
            subtable.codepoints(|codepoint| {
                if let Some(glyph) = char::from_u32(codepoint).and_then(|c| face.glyph_index(c)) {
                    mappings.insert(codepoint, glyph.0 as u32);
                }
            });
        }
    }
    mappings.extend(digits);
    let mut groups: Vec<(u32, u32, u32)> = vec![];
    for (codepoint, glyph) in mappings {
        if let Some((first, last, start_glyph)) = groups.last_mut()
            && codepoint == *last + 1
            && glyph == *start_glyph + codepoint - *first
        {
            *last = codepoint;
        } else {
            groups.push((codepoint, codepoint, glyph));
        }
    }
    let mut cmap = vec![];
    for value in [0_u16, 1, 3, 10] {
        cmap.extend_from_slice(&value.to_be_bytes());
    }
    cmap.extend_from_slice(&12_u32.to_be_bytes());
    for value in [12_u16, 0] {
        cmap.extend_from_slice(&value.to_be_bytes());
    }
    for value in [16 + 12 * groups.len() as u32, 0, groups.len() as u32] {
        cmap.extend_from_slice(&value.to_be_bytes());
    }
    for (first, last, glyph) in groups {
        for value in [first, last, glyph] {
            cmap.extend_from_slice(&value.to_be_bytes());
        }
    }
    Some(cmap)
}

#[cfg(target_os = "macos")]
pub fn system_font(mono: bool) -> Option<&'static [u8]> {
    use std::sync::OnceLock;
    static SANS: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    static MONO: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    (if mono { &MONO } else { &SANS })
        .get_or_init(|| {
            let mut bytes = std::fs::read(if mono {
                "/System/Library/Fonts/SFNSMono.ttf"
            } else {
                "/System/Library/Fonts/SFNS.ttf"
            })
            .ok()?;
            if !mono {
                tabular_digits(&mut bytes, 0);
            }
            Some(bytes)
        })
        .as_deref()
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[test]
    fn tabular_mapping_preserves_other_glyphs_and_equalizes_digits() {
        let original = std::fs::read("/System/Library/Fonts/SFNS.ttf").unwrap();
        let mut bytes = original.clone();
        tabular_digits(&mut bytes, 0);
        let before = Face::parse(&original, 0).unwrap();
        let after = Face::parse(&bytes, 0).unwrap();
        let advance = after.glyph_hor_advance(after.glyph_index('0').unwrap());
        for digit in '0'..='9' {
            assert_eq!(after.glyph_hor_advance(after.glyph_index(digit).unwrap()), advance);
        }
        for text in ['A', 'é', '→', '⌘'] {
            assert_eq!(before.glyph_index(text), after.glyph_index(text));
        }
        assert_eq!(checksum(&bytes), 0xb1b0_afba);
    }
}
