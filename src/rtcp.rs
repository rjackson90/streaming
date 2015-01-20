use std::rand;
use std::cmp;
use std::collections::HashMap;

use super::{Ssrc, Csrc};

#[allow(dead_code)]
enum PacketType {
    SendReport,
    ReceiveReport,
    SourceDescription,
    Bye,
    App,
    Rtp
}

#[allow(dead_code)]
enum MemberState {
    Listening,
    Sending,
    Bye
}

#[allow(dead_code)]
struct Member {
    id: Ssrc,
    cname: Option<String>,
    status: Option<MemberState>,
    intervals: i32  // TX intervals since last packet seen
}

#[allow(dead_code)]
struct State {
    tp: f32,            // The last time an RTCP packet was transmitted
    tc: f32,            // Current time
    tn: f32,            // Next scheduled transmission time
    pmembers: i32,      // Previous estimate of member count
    members: i32,       // Current estimate of member count
    senders: i32,       // Current estimate of sender count
    rtcp_bw: i32,       // Target RTCP bandwidth, in octets per second
    we_sent: bool,      // Flag: True if application sent data recently
    avg_rtcp_size: f32, // Average compound RTCP packet size, in octets
    initial: bool,      // Flag: True if a packet has not yet been sent
    member_table: HashMap<Ssrc, Member> // A List of all members of the current session
}

impl State {
    
    /// Initializes an RTCP session
    ///
    /// There isn't any handshaking involved with setting up an RTCP session, 
    /// so this method mainly consists of initializing a new State object.
    /// The application is responsible for providing a few pieces of important
    /// information, such as its SSRC, the available bandwidth, and the expected 
    /// size of the first packet. See section 6.3.2 of [RFC 3550]
    /// (tools.ietf.org/html/rfc3550#section-6.3.2) for more information.
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
    pub fn initialize(our_ssrc: Ssrc, bandwidth: i32, pkt_size: i32) -> State {
        let mut result = State {
            tp: 0.0,
            tc: 0.0,
            tn: 0.0, // Dummy value, to be recalculated after struct initialized
            pmembers: 1,
            members: 1,
            senders: 0,
            rtcp_bw: bandwidth,
            we_sent: false,
            avg_rtcp_size: pkt_size as f32,
            initial: true,
            member_table: HashMap::with_capacity(32)
        };

        // Add ourselves to the member table as a listener
        result.member_table.insert(our_ssrc, 
                                   Member { id: our_ssrc, cname: None, 
                                            status: Some(MemberState::Listening),
                                            intervals: 0});
        
        // Calculate the initial tx interval and return
        result.tn = result.tx_interval();
        result
    }

    /// Computes the RTCP Transmission Interval based on the current session state.
    ///
    /// The time interval between transmissions of RTCP packets varies with the number
    /// of members in the current session in order to avoid congestion. The value is
    /// random, but gives on average 25% of RTCP bandwidth to senders. The time
    /// interval is calculated as described in section 6.3.1 of [RFC 3550]
    /// (tools.ietf.org/html/rfc3550#section-6.3.1).
    ///
    /// # Return value
    ///
    /// The return value is the time interval between RTCP packets, in seconds.
    #[allow(unstable)]
    pub fn tx_interval(&self) -> f32 {
        
        let few_senders = self.senders as f32 <= 0.25 * self.members as f32;

        let c_times_n = match few_senders {
            true    => {
                match self.we_sent {
                    true    => {
                        (self.avg_rtcp_size / self.rtcp_bw as f32 * 0.25) * // C
                        self.senders as f32 // n
                    },
                    
                    false   => {
                        (self.avg_rtcp_size / self.rtcp_bw as f32 * 0.75) * // C
                        (self.members - self.senders) as f32 // n
                    },
                }
            },
            
            false   => {
                (self.avg_rtcp_size / self.rtcp_bw as f32) * // C
                self.members as f32 // n
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

    #[allow(dead_code)]
    #[allow(unused_variables)]
    pub fn pkt_recv_notify(&mut self, packet_type: PacketType, packet_size: f32, 
                       ssrc: Ssrc, csrcs: &[Csrc]) {
        match packet_type{
            PacketType::Bye => {
                match self.member_table.get_mut(&ssrc) {
                    None        => (), // Ignore BYE for members not in table
                    Some(member)=> {
                        match member.status {
                            Some(MemberState::Listening) => {
                                member.status = Some(MemberState::Bye);
                                self.members -= 1;
                            },

                            Some(MemberState::Sending)    => {
                                member.status = Some(MemberState::Bye);
                                self.members -= 1;
                                self.senders -= 1;
                            },

                            _ => () // Ignore duplicate BYEs and unvalidated members
                        }
                    }
                }

                // "reverse reconsideration" algorithm as per RFC 3550 6.3.4
                if self.members < self.pmembers {
                    self.tn = self.tc + (self.members as f32 / self.pmembers as f32) * 
                              (self.tn - self.tc);
                    
                    self.tp = self.tc - (self.members as f32 / self.pmembers as f32) * 
                              (self.tc - self.tp);
                    
                    self.pmembers = self.members;
                }
            },

            PacketType::Rtp => {
                self.update_member_status(ssrc, true);
                for &ident in csrcs.iter() {
                    self.update_member_status(ident, false);
                }
            },
            
            _   => {
                self.update_member_status(ssrc, false);
                for &ident in csrcs.iter() {
                    self.update_member_status(ident, false);
                }
            },
        }

        self.avg_rtcp_size = (1.0 / 16.0) * packet_size + (15.0 / 16.0) * 
                             self.avg_rtcp_size;
    }

    #[allow(dead_code)]
    fn update_member_status(&mut self, id: Ssrc, is_sender: bool) {
        let exists: bool;
        match self.member_table.get_mut(&id)  {
            None            => exists = false,
            Some(member)    => {
                exists = true;
                member.intervals = 0;   // Member is in the table, mark as seen
                match member.status {
                    None    => {
                        member.status = Some(MemberState::Listening); // Validate member
                        self.members += 1;
                    },
                    _       => (), // Member has already been validated
                }
            },
        };

        if !exists{
            // Member is not in the table. Providing a None status 
            // indicates first contact
            if !is_sender {
                self.member_table.insert(id, Member {id: id, cname: None, 
                                                     status: None, intervals: 0});
            } else {
                // Senders are validated immediately
                self.member_table.insert(id, 
                                         Member {id: id, cname: None,
                                                 status: Some(MemberState::Sending),
                                                 intervals: 0});
                self.members += 1;
                self.senders += 1;
            }
        }       
    }
}
