#![deny(warnings)]
#![cfg_attr(clippy, allow(cyclomatic_complexity))]
// false positive on `unwrap_or_else(|e| parse::bug(e))` because divergent functions don't implement
// the `Fn` traits.
#![cfg_attr(clippy, allow(redundant_closure))]

extern crate clap;
extern crate mpd;

use std::borrow::Cow;
use std::{io, process};

use clap::{App, Arg, Format, SubCommand};
use mpd::{Connection, Command, Extra, Mode, Song, State, Status, parse};

fn main() {
    // TODO report I/O errors
    run().unwrap();
}

fn run() -> io::Result<()> {
    // Possible values for boolean arguments
    static VALUES: &'static [&'static str] = &["0", "1", "false", "no", "off", "on", "true", "yes"];

    let matches = &App::new("mpc")
                          .arg(Arg::with_name("quiet")
                                   .help("Suppress status message")
                                   .long("quiet")
                                   .short("q"))
                          .subcommand(SubCommand::with_name("add")
                                          .about("Add song to the current playlist")
                                          .arg(Arg::with_name("uri").required(true)))
                          .subcommand(SubCommand::with_name("clear")
                                          .about("Clear the current playlist"))
                          .subcommand(SubCommand::with_name("consume")
                                          .about("Set consume mode")
                                          .arg(Arg::with_name("state")
                                                   .possible_values(VALUES)
                                                   .required(true)))
                          .subcommand(SubCommand::with_name("listall")
                                          .about("List all songs in the music dir")
                                          .arg(Arg::with_name("uri")))
                          .subcommand(SubCommand::with_name("next")
                                          .about("Play the next song in the current playlist"))
                          .subcommand(SubCommand::with_name("pause")
                                          .about("Pauses the currently playing song"))
                          .subcommand(SubCommand::with_name("play")
                                          .about("Start playing at <position>")
                                          .arg(Arg::with_name("position")))
                          .subcommand(SubCommand::with_name("playlist")
                                          .about("Prints the current playlist"))
                          .subcommand(SubCommand::with_name("prev")
                                          .about("Play the previous song in the current playlist"))
                          .subcommand(SubCommand::with_name("random")
                                          .about("Set random mode")
                                          .arg(Arg::with_name("state")
                                                   .possible_values(VALUES)
                                                   .required(true)))
                          .subcommand(SubCommand::with_name("repeat")
                                          .about("Set repeat mode")
                                          .arg(Arg::with_name("state")
                                                   .possible_values(VALUES)
                                                   .required(true)))
                          .subcommand(SubCommand::with_name("single")
                                          .about("Set single mode")
                                          .arg(Arg::with_name("state")
                                                   .possible_values(VALUES)
                                                   .required(true)))
                          .subcommand(SubCommand::with_name("stop")
                                          .about("Stop the currently playing playlist"))
                          .subcommand(SubCommand::with_name("update")
                                          .about("Scan music directory for updates")
                                          .arg(Arg::with_name("uri")))
                          .subcommand(SubCommand::with_name("version")
                                          .about("Report version of MPD"))
                          .subcommand(SubCommand::with_name("volume")
                                          .about("Set volume")
                                          .arg(Arg::with_name("level").required(true)))
                          .get_matches();

    let conn_opt = &mut None;
    let mut quiet = matches.is_present("quiet");

    let subcommand = matches.subcommand();

    if !subcommand.0.is_empty() {
        let conn = try!(connect(conn_opt));

        match subcommand {
            // Boolean commands
            (mode @ "consume", Some(matches)) |
            (mode @ "random", Some(matches)) |
            (mode @ "repeat", Some(matches)) |
            (mode @ "single", Some(matches)) => {
                fn parse(value: &str) -> bool {
                    match value {
                        "0" | "false" | "no" | "off" => false,
                        "1" | "on" | "true" | "yes" => true,
                        _ => unreachable!(),
                    }
                }

                let mode = match mode {
                    "consume" => Mode::Consume,
                    "random" => Mode::Random,
                    "repeat" => Mode::Repeat,
                    "single" => Mode::Single,
                    _ => unreachable!(),
                };

                try!(conn.send(Command::Set {
                    mode: mode,
                    state: matches.value_of("state").map(parse).unwrap(),
                }));
                try!(conn.recv());
            }
            // Commands with no arguments
            (cmd @ "clear", _) |
            (cmd @ "next", _) |
            (cmd @ "pause", _) |
            (cmd @ "prev", _) |
            (cmd @ "stop", _) => {
                let cmd = match cmd {
                    "clear" => Command::Clear,
                    "next" => Command::Next,
                    "pause" => Command::Pause { state: true },
                    "prev" => Command::Previous,
                    "stop" => Command::Stop,
                    _ => unreachable!(),
                };

                try!(conn.send(cmd));
                try!(conn.recv());
            }
            // Commands with a single required argument
            ("add", Some(matches)) => {
                try!(conn.send(Command::Add { uri: matches.value_of("uri").unwrap() }));
                try!(conn.recv());
            }
            ("volume", Some(matches)) => {
                try!(conn.send(Command::Volume {
                    level: matches.value_of("level").and_then(|s| s.parse().ok()).unwrap(),
                }));
                try!(conn.recv());
            }
            // Commands with a single optional arguments
            ("update", Some(matches)) => {
                try!(conn.send(Command::Update { uri: matches.value_of("uri") }));
                try!(conn.recv());
            }
            // Command::Play has a special argument restriction (> 0)
            ("play", Some(matches)) => {
                try!(conn.send(Command::Play {
                    position: matches.value_of("position").map(|s| {
                        s.parse::<u32>()
                         .ok()
                         .and_then(|i| i.checked_sub(1))
                         .unwrap_or_else(|| invalid_value(s, matches.usage()))
                    }),
                }));
                try!(conn.recv());
            }
            // the version subcommand doesn't map to a MPD command
            ("version", _) => {
                quiet = true;

                let version = conn.version();
                println!("mpd version: {}.{}.{}",
                         version.major(),
                         version.minor(),
                         version.patch());
            }
            ("listall", Some(matches)) => {
                quiet = true;

                try!(conn.send(Command::ListAll { uri: matches.value_of("uri") }));

                for line in try!(conn.recv()).lines() {
                    const FILE: &'static str = "file: ";

                    if line.starts_with(FILE) {
                        println!("{}", &line[FILE.len()..]);
                    }
                }
            }
            ("playlist", _) => {
                quiet = true;

                try!(conn.send(Command::PlaylistInfo));
                let mut text = try!(conn.recv());

                if !text.trim().is_empty() {
                    while let Some(end) = text.find("\nfile:") {
                        let song = Song::parse(&text[..end]).unwrap_or_else(|e| parse::bug(e));
                        println!("{} - {}", song.artist, song.title);
                        text = &text[end + 1..];
                    }

                    let song = Song::parse(text).unwrap_or_else(|e| parse::bug(e));
                    println!("{} - {}", song.artist, song.title);
                }

            }
            _ => {}
        }
    }

    if !quiet {
        try!(status(try!(connect(conn_opt))));
    }

    Ok(())
}

/// Encountered an invalid value, print an error message and exit
fn invalid_value(value: &str, usage: &str) -> ! {
    println!("{} '{}' isn't a valid value\n\n{}\n\nPlease re-run with {} for more information",
             Format::Error("error:"),
             Format::Warning(value),
             usage,
             Format::Good("--help"));
    process::exit(1);
}

/// Connects to MPD if not yet connected, otherwise returns the current connection
fn connect(conn_opt: &mut Option<Connection>) -> io::Result<&mut Connection> {
    Ok(if let Some(ref mut conn) = *conn_opt {
        conn
    } else {
        *conn_opt = Some(try!(mpd::connect("localhost:6600")));
        conn_opt.as_mut().unwrap()
    })
}

/// Prints status information
fn status(conn: &mut Connection) -> io::Result<()> {
    fn onoff(on: bool) -> &'static str {
        if on {
            "on "
        } else {
            "off"
        }
    }

    try!(conn.send(Command::Status));
    let status = Status::parse(try!(conn.recv())).unwrap_or_else(|e| parse::bug(e));

    let state = match status.state {
        State::Pause => Some("paused"),
        State::Play => Some("playing"),
        State::Stop => None,
    };

    if let (Some(state), Some(Extra { pos, time: Some(ref time), .. })) = (state, status.extra) {
        try!(conn.send(Command::CurrentSong));

        let song = Song::parse(try!(conn.recv())).unwrap_or_else(|e| parse::bug(e));

        println!("{} - {}", song.artist, song.title);
        println!("[{}] #{}/{}   {}:{:02}/{}:{:02} ({}%)",
                 state,
                 pos + 1,
                 status.playlist_length,
                 time.elapsed / 60,
                 time.elapsed % 60,
                 time.total / 60,
                 time.total % 60,
                 100 * time.elapsed / time.total);
    }

    if let Some(id) = status.updating_db {
        println!("Updating DB (#{}) ...", id);
    }

    println!("volume: {}   repeat: {}   random: {}   single: {}   consume: {}",
             status.volume.map(|n| Cow::from(format!("{}%", n))).unwrap_or(Cow::from("n/a")),
             onoff(status.repeat),
             onoff(status.random),
             onoff(status.single),
             onoff(status.consume));

    Ok(())
}
