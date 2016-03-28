//! Library to interface MPD

// Developers: keep handy: http://www.musicpd.org/doc/protocol/command_reference.html

#![deny(missing_docs)]
#![deny(warnings)]

extern crate bufstream;

use std::borrow::Cow;
use std::io::{self, BufRead, Write};
use std::net::{TcpStream, ToSocketAddrs};

use bufstream::BufStream;

pub mod parse;

/// MPD status
#[derive(PartialEq)]
pub enum State {
    /// Playing
    Play,
    /// Paused
    Pause,
    /// Stapped
    Stop,
}

#[allow(missing_docs)]
/// Song information
pub struct Song<'a> {
    // TODO parse other fields
    _0: (),
    pub artist: &'a str,
    pub title: &'a str,
}

/// Elapsed and total time
pub struct Time {
    _0: (),
    /// Elapsed time in seconds
    pub elapsed: u32,
    /// Total time in seconds
    pub total: u32,
}

/// Extra information about currently playing song
pub struct Extra {
    _0: (),
    /// Elapsed time, with higher precision
    pub elapsed: Option<f64>,
    /// Song position
    pub pos: u32,
    /// Elapsed/total time
    pub time: Option<Time>,
}

/// MPD status
pub struct Status {
    // TODO parse other fields
    _0: (),
    /// State of the consume mode
    pub consume: bool,
    /// Extra information, available only when a song being played
    pub extra: Option<Extra>,
    /// Length of the playlist
    pub playlist_length: u32,
    /// State of the random mode
    pub random: bool,
    /// State of the repeat mode
    pub repeat: bool,
    /// State of the single mode
    pub single: bool,
    /// MPD state
    pub state: State,
    /// `Some` variant indicates there is a db update job running, and contains its job id
    pub updating_db: Option<u32>,
    /// Volume level. `None` indicates that MPD can't control the volume level
    pub volume: Option<u8>,
}

#[allow(missing_docs)]
/// MPD mode
pub enum Mode {
    /// When consume mode is activated, each song played is removed from playlist
    Consume,
    Random,
    Repeat,
    /// When single mode is activated, playback is stopped after current song, or song is repeated
    /// if the 'repeat' mode is enabled
    Single,
}

impl Mode {
    fn str(&self) -> &'static str {
        use self::Mode::*;

        match *self {
            Consume => "consume",
            Random => "random",
            Repeat => "repeat",
            Single => "single",
        }
    }
}

/// A MPD command
pub enum Command<'a> {
    /// Adds the file `uri` to the playlist (directories are added recursively)
    Add {
        /// If `None`, adds the whole database
        uri: &'a str,
    },
    /// Clears the current playlist
    Clear,
    /// Displays the song info of the current song
    CurrentSong,
    /// Lists all songs and directories in `uri`
    ListAll {
        /// If `None`, list everything in the database
        uri: Option<&'a str>,
    },
    /// Plays next song in the playlist
    Next,
    /// Toggles pause/resumes playing
    Pause {
        /// `true`: pauses, `false`: resume playing
        state: bool,
    },
    /// Begins playing the playlist at song `position`
    Play {
        /// if `None`, resumes playing the current song
        position: Option<u32>,
    },
    /// Displays a list of all songs in the playlist
    PlaylistInfo,
    /// Plays previous song in the playlist
    Previous,
    /// Sets `mode` to `state`
    Set {
        /// MPD mode
        mode: Mode,
        /// `true`: mode enabled, `false`: mode disabled
        state: bool,
    },
    /// Reports the current status of the player and the volume level
    Status,
    /// Stops playing
    Stop,
    /// Updates the music database. `uri` is a particular directory or file to update.
    Update {
        /// If `None`, updates everything
        uri: Option<&'a str>,
    },
    /// Sets volume level
    Volume {
        /// volume level
        level: u32,
    },
}

impl<'a> Command<'a> {
    fn str(&self) -> Cow<'static, str> {
        use self::Command::*;

        Cow::from(match *self {
            Add { uri } => return format!("add \"{}\"", uri).into(),
            Clear => "clear",
            CurrentSong => "currentsong",
            ListAll { uri: None } => "listall",
            ListAll { uri: Some(uri) } => return format!("listall \"{}\"", uri).into(),
            Next => "next",
            Pause { state: false } => "pause 0",
            Pause { state: true } => "pause 1",
            Play { position: None } => "play",
            Play { position: Some(pos) } => return format!("play {}", pos).into(),
            PlaylistInfo => "playlistinfo",
            Previous => "previous",
            Set { ref mode, state } => {
                return format!("{} {}",
                               mode.str(),
                               if state {
                                   "1"
                               } else {
                                   "0"
                               })
                           .into()
            }
            Status => "status",
            Stop => "stop",
            Update { uri: None } => "update",
            Update { uri: Some(uri) } => return format!("update \"{}\"", uri).into(),
            Volume { level } => return format!("setvol {}", level).into(),
        })
    }
}

/// A connection to MPD
pub struct Connection {
    buffer: String,
    stream: BufStream<TcpStream>,
    version: Version,
}

impl Connection {
    /// Sends a command to MPD
    pub fn send(&mut self, cmd: Command) -> io::Result<()> {
        let ref mut stream = self.stream;
        try!(writeln!(stream, "{}", cmd.str()));
        stream.flush()
    }

    /// Returns command output
    pub fn recv(&mut self) -> io::Result<&str> {
        let Connection { ref mut buffer, ref mut stream, .. } = *self;

        buffer.clear();

        try!(stream.read_line(buffer));

        if buffer.starts_with("ACK") {
            // TODO lift error
            panic!("BUG: unhandled server error: {}", buffer.trim_right());
        } else {
            // End Of Message
            const EOM: &'static str = "OK\n";

            while !buffer.ends_with(EOM) {
                try!(stream.read_line(buffer));
            }

            Ok(buffer[..buffer.len() - EOM.len()].trim_right())
        }
    }

    /// Returns MPD version
    pub fn version(&self) -> &Version {
        &self.version
    }
}

/// MPD version
pub struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

impl Version {
    fn parse(input: &str) -> Result<Version, ()> {
        let ref mut parts = input.splitn(3, ".");

        let major = try!(try!(parts.next().ok_or(())).parse().map_err(|_| ()));
        let minor = try!(try!(parts.next().ok_or(())).parse().map_err(|_| ()));
        let patch = try!(try!(parts.next().ok_or(())).parse().map_err(|_| ()));

        Ok(Version {
            major: major,
            minor: minor,
            patch: patch,
        })
    }

    /// Returns the major component of the version
    pub fn major(&self) -> u32 {
        self.major
    }

    /// Returns the minor component of the version
    pub fn minor(&self) -> u32 {
        self.minor
    }

    /// Returns the patch component of the version
    pub fn patch(&self) -> u32 {
        self.patch
    }
}

/// Connects to the MPD with address `addr`
pub fn connect<A>(addr: A) -> io::Result<Connection>
    where A: ToSocketAddrs
{
    fn new(stream: TcpStream) -> io::Result<Connection> {
        let mut stream = BufStream::new(stream);
        let mut buffer = String::new();

        try!(stream.read_line(&mut buffer));

        if !buffer.starts_with("OK MPD ") {
            // TODO lift error
            panic!("BUG: unhandled server error: expected 'OK MPD {{version}}' got '{}'",
                   buffer)
        }

        let version = {
            let version = &buffer["OK MPD ".len()..].trim_right();
            Version::parse(version).unwrap_or_else(|_| {
                panic!("BUG: error parsing '{}' as Version", version);
            })
        };

        buffer.clear();
        Ok(Connection {
            buffer: buffer,
            stream: stream,
            version: version,
        })
    }

    new(try!(TcpStream::connect(addr)))
}
