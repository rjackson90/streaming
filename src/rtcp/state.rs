extern crate time;

use self::time::SteadyTime;

use std::rand::{random, Closed01};
use std::cmp;
use std::collections::HashMap;
use std::time::Duration;
use std::i64;

use super::super::{Ssrc, Csrc};

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
    tp: SteadyTime,     // The last time an RTCP packet was transmitted
    tc: SteadyTime,     // Current time
    tn: SteadyTime,     // Transmission interval
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
            tp: SteadyTime::now(),
            tc: SteadyTime::now(),
            tn: SteadyTime::now(), // Dummy value, to be recalculated later
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
        result.tn = result.tc + result.tx_interval();
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
    pub fn tx_interval(&self) -> Duration {
        
        let few_senders = self.senders as f32 <= 0.25 * self.members as f32;

        let c_times_n = match few_senders {
            true => {
                match self.we_sent {
                    true => {
                        Duration::microseconds((self.avg_rtcp_size as f32 / 
                                                self.rtcp_bw as f32 * 
                                                0.25 * 
                                                1000000.0 ) as i64 * 
                                               self.senders as i64)
                    },
                    
                    false => {
                        Duration::microseconds((self.avg_rtcp_size as f32 /
                                                self.rtcp_bw as f32 *
                                                0.75 *
                                                1000000.0 ) as i64 *
                                               (self.members - self.senders) as i64)
                    },
                }
            },
            
            false => {
                Duration::microseconds((self.avg_rtcp_size as f32 /
                                        self.rtcp_bw as f32 *
                                        1000000.0 ) as i64 *
                                       self.members as i64)
            },
        };

        let t_min = if self.initial {
            Duration::milliseconds(2500)
        } else {
            Duration::milliseconds(5000)
        };
        
        let t_d = cmp::max(t_min, c_times_n);

        let t_d_micros = match t_d.num_microseconds() {
            Some(micros)=> micros,
            None        => i64::MAX // Assumption: None is always an overflow
        };

        let Closed01(rand) = random::<Closed01<f64>>();

        let t_rand = ( t_d_micros as f64 / 2.0 ) + 
                     ( rand * t_d_micros as f64 );
        Duration::microseconds((t_rand / 1.21828) as i64)
    }

    #[allow(dead_code)]
    #[allow(unused_variables)]
    pub fn pkt_recv_notify(&mut self, packet_type: PacketType, packet_size: i32, 
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

                            Some(MemberState::Sending) => {
                                member.status = Some(MemberState::Bye);
                                self.members -= 1;
                                self.senders -= 1;
                            },

                            _ => () // Ignore duplicate BYEs and unvalidated members
                        }
                    }
                }

                self.reverse_reconsideration();
            },

            PacketType::Rtp => {
                self.update_member_status(ssrc, true);
                for &ident in csrcs.iter() {
                    self.update_member_status(ident, false);
                }
            },
            
            _ => {
                self.update_member_status(ssrc, false);
                for &ident in csrcs.iter() {
                    self.update_member_status(ident, false);
                }
            },
        }

        self.avg_rtcp_size = self.update_avg_packet_size(packet_size);
    }

    fn reverse_reconsideration(&mut self) {
        if self.members < self.pmembers {
            self.tn = self.tc + ((self.tn - self.tc) * (self.members / self.pmembers));

            self.tp = self.tc - ((self.tc - self.tp) * (self.members / self.pmembers));

            self.pmembers = self.members;
        }
    }

    #[allow(dead_code)]
    fn update_member_status(&mut self, id: Ssrc, is_sender: bool) {
        let exists: bool;
        match self.member_table.get_mut(&id)  {
            None => exists = false,
            Some(member) => {
                exists = true;
                member.intervals = 0;   // Member is in the table, mark as seen
                match member.status {
                    None => {
                        member.status = Some(MemberState::Listening); // Validate member
                        self.members += 1;
                    },
                    _ => (), // Member has already been validated
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

    #[allow(dead_code)]
    fn tx_timer_expire(&mut self) {
        let t = self.tx_interval();

        match (self.tp + t).cmp(&self.tc) {
            cmp::Ordering::Less | cmp::Ordering::Equal => {
                // TODO: signal the host application to send a packet

                self.tp = self.tc;
                self.tn = self.tc + self.tx_interval();
            },
            
            cmp::Ordering::Greater => {
                self.tn = self.tp + t;
            }
        };

        // TODO: set transmission timer to expire at time tn


        self.pmembers = self.members;
    }

    #[allow(unused_variables)]
    #[allow(dead_code)]
    fn pkt_send_notify(&mut self, packet_type: Option<PacketType>, packet_size: i32,
                       our_ssrc: Ssrc) {
        self.initial = false;
        self.avg_rtcp_size = self.update_avg_packet_size(packet_size);

        match packet_type {
            None => (),
            Some(p_type) => match p_type {
                PacketType::Rtp => {
                    if self.we_sent == false {
                        self.we_sent = true;
                        self.senders += 1;
                    }
                    
                    let sender = self.member_table.get_mut(&our_ssrc).expect(
                                     "This machine is not in the member table!");
                    sender.intervals = 0;

                }
                _ => ()
            }
        };

        self.reverse_reconsideration();
    }

    fn update_avg_packet_size(&self, size: i32) -> f32 {
        (1.0 / 16.0) * size as f32 + (15.0 / 16.0) * self.avg_rtcp_size
    }

    #[allow(dead_code)]
    fn leave_session(&mut self) {
        if self.members >= 50 {
            // This implementation is designed around having <10 members or so at
            // all times. It's not worth implementing the BYE backoff algorithm
            // from RFC 3550 6.3.7 for such a rare case, so if there are a lot
            // of members in this session, no BYE will be transmitted and departing
            // members will simply time out. 
        }
        
        // TODO: signal the host application to send a BYE
    }
}
