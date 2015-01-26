

use super::super::Ssrc;

static RTP_VERSION: u8 = 10b;
static SENDER_TYPE: u8 = 200;
static RECEIVER_TYPE: u8 = 201;

struct Header {
    version: u8,        // RTP version 2 (2 bits)
    padding: bool,      // indicates the presence of padding for encryption (1 bit)
    report_count: u8,   // The number of report blocks in this packet (5 bits)
    packet_type: u8,    // Constant value to identify this packet as a SR packet (8 bits)
    length: u16,        // Length of this packet + header measured in words - 1. (16 bits)
    ssrc: Ssrc          // The SSRC of this machine (32 bits)
}

struct SenderInfo {
    ntp_time: u64,      // NTP wallclock timestamp (64 bits)
    rtp_time: u32,      // RTP timestamp, very similar to the NTP timestamp (32 bits)
    packet_count: u32,  // Total number of RTP packets transmitted by this sender (32 bits)
    octet_count: u32,   // Total number of payload octets transmitted (32 bits)
}

struct ReportBlock {
    ssrc: Ssrc,         // The SSRC of the source to which this block pertains (32 bits)
    lost: u8,           // Fraction of RTP packets lost since the previous report (8 bits)
    lost_total: u32,    // Total number of lost RTP packets from this source (24 bits)
    highest_seq: u32,   // Highest sequence number received in an RTP packet (32 bits)
    jitter: u32,        // Estimate of interarrival jitter from this source (32 bits)
    last_sr: u32,       // Time of last SR received from this source
    sr_delay: u32,      // Delay between the last SR and sending this RR (32 bits)
}

