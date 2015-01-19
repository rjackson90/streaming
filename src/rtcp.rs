use std::rand;
use std::cmp;

use super::Ssrc;

#[allow(dead_code)]
enum PacketType {
    SR,
    RR,
    SDES,
    BYE,
    APP
}

#[allow(dead_code)]
struct State {
    tp: i32,            // The last time an RTCP packet was transmitted
    tc: i32,            // Current time
    tn: f32,            // Next scheduled transmission time
    pmembers: i32,      // Previous estimate of member count
    members: i32,       // Current estimate of member count
    senders: i32,       // Current estimate of sender count
    rtcp_bw: i32,       // Target RTCP bandwidth, in octets per second
    we_sent: bool,      // Flag: True if application sent data recently
    avg_rtcp_size: i32, // Average compound RTCP packet size, in octets
    initial: bool,      // Flag: True if a packet has not yet been sent
    member_table: Vec<Ssrc> // A List of all members of the current session
}

impl State {
    
    /// Initializes an RTCP session
    ///
    /// There isn't any handshaking involved with setting up an RTCP session, 
    /// so this method mainly consists of initializing a new State object.
    /// The application is responsible for providing a few pieces of important
    /// information, such as its SSRC, the available bandwidth, and the expected 
    /// size of the first packet.
    ///
    /// # Arguments
    ///
    /// * `our_ssrc` - The application's SSRC, used to uniquely identify this
    ///                synchronization source to participants in the session.
    /// * `bandwidth` - The fraction of session bandwidth available to *all* RTCP
    ///                 participants, in octets per second. This quantity
    ///                 is fixed during startup.
    /// * `pkt_size` - Best guess as to the size of the first RTCP packet which
    ///                will be later constructed. This can be off a bit, but it
    ///                helps to be close. 
    ///
    /// # Return Value
    ///
    /// The returned State object holds the state of the current RTCP session. 
    #[allow(dead_code)]
    fn initialize(our_ssrc: Ssrc, bandwidth: i32, pkt_size: i32) -> State {
        let mut result = State {
            tp: 0,
            tc: 0,
            tn: 0.0, // Dummy value, to be recalculated after struct initialized
            pmembers: 1,
            members: 1,
            senders: 0,
            rtcp_bw: bandwidth,
            we_sent: false,
            avg_rtcp_size: pkt_size,
            initial: true,
            member_table: Vec::with_capacity(32)
        };

        result.member_table.push(our_ssrc);
        result.tn = result.tx_interval();
        
        result
    }

    
    /// Computes the RTCP Transmission Interval based on the current session state.
    ///
    /// The time interval between transmissions of RTCP packets varies with the number
    /// of members in the current session in order to avoid congestion. The value is
    /// random, but gives on average 25% of RTCP bandwidth to senders. The time
    /// interval is calculated as described in section 6.3.1 of [RFC 3550]
    /// (tools.ietf.org/html/rfc3550).
    ///
    /// # Return value
    ///
    /// The return value is the time interval between RTCP packets, in seconds.
    #[allow(dead_code)]
    #[allow(unstable)]
    fn tx_interval(&self) -> f32 {
        
        let few_senders = self.senders as f32 <= 0.25 * self.members as f32;

        let c_times_n = match few_senders {
            true    => {
                match self.we_sent {
                    true    => {
                        (self.avg_rtcp_size as f32 / self.rtcp_bw as f32 * 0.25) * // C
                        self.senders as f32 // n
                    },
                    
                    false   => {
                        (self.avg_rtcp_size as f32 / self.rtcp_bw as f32 * 0.75) * // C
                        (self.members - self.senders) as f32 // n
                    },
                }
            },
            
            false   => {
                (self.avg_rtcp_size as f32 / self.rtcp_bw as f32) * // C
                self.members as f32                                 // n
            },
        };

        let t_min = if self.initial {2.5} else {5.0};
        
        let t_d = match cmp::partial_max(t_min, c_times_n) {
            Some(max)   => max,
            None        => t_min
        };

        let t_rand = (0.5 * t_d) + (rand::random::<f32>() * t_d);
        t_rand / 1.21828
    }
}
