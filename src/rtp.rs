use super::{Word, Csrc, Ssrc};

#[allow(dead_code)]
struct Header {
    // 1st word
    version: u8,            // Bits 0-1
    padding: bool,          // Bit 2
    extension: bool,        // Bit 3
    csrc_count: u8,         // Bits 4-7
    marker: bool,           // Bit 8
    payload_type: u8,       // Bits 9-15
    sequence_number: u16,   // Bits 16-31

    // 2nd word    
    timestamp: Word,
    
    // 3rd word
    sync_src_id: Ssrc,
    
    // There can be 0-15 CSRCs, each fills a word
    csrc_ids: Vec<Csrc>,
}
