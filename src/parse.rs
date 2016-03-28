//! Parsing

use std::str::FromStr;

use {Extra, Song, State, Status, Time};

macro_rules! parse_ty {
    ($e:expr, $ty:ty) => {
        parse_ty::<$ty>($e, stringify!($ty))
    }
}

#[allow(missing_docs)]
/// Parse error
pub enum Error<'a> {
    /// Expected to find a certain `key` in `lines`
    ExpectedKey {
        key: &'static str,
        lines: &'a str,
    },
    /// Missing the "{key}" part of "{key}: {value}" in `line`
    MissingKey {
        line: &'a str,
    },
    /// Missing the "{value}" part of "{key}: {value}" in `line`
    MissingValue {
        line: &'a str,
    },
    /// Couldn't parse `value` as type `ty`
    ParseType {
        ty: &'static str,
        value: &'a str,
    },
    /// A certain "{key}: {value}" line wasn't handled by the parser
    UnhandledKeyValuePair {
        key: &'a str,
        value: &'a str,
    },
}

/// Treats a parse error as a bug and panics
pub fn bug(e: Error) -> ! {
    use self::Error::*;

    match e {
        ExpectedKey { key, lines } => panic!("BUG: Expected to find key {} in:\n{}", key, lines),
        MissingKey { line } => {
            panic!("BUG: Missing {{key}} when parsing {:?} as \"{{key}}: {{value}}\"",
                   line)
        }
        MissingValue { line } => {
            panic!("BUG: Missing {{value}} when parsing {:?} as \"{{key}}: {{value}}\"",
                   line)
        }
        ParseType { ty, value } => panic!("BUG: Couldn't parse {} as {}", value, ty),
        UnhandledKeyValuePair { key, value } => {
            panic!("BUG: Unhandled key-value pair: ({}, {})", key, value)
        }
    }
}

/// Parses `value` as a boolean represented as "0" or "1"
fn parse_bool(value: &str) -> Result<bool, Error> {
    Ok(match value {
        "0" => false,
        "1" => true,
        _ => {
            return Err(Error::ParseType {
                ty: "bool",
                value: value,
            })
        }
    })
}

/// Parses `value` as `T`, `ty` must match the name of `T`
fn parse_ty<'a, T>(value: &'a str, ty: &'static str) -> Result<T, Error<'a>>
    where T: FromStr
{
    value.parse::<T>().map_err(|_| {
        Error::ParseType {
            ty: ty,
            value: value,
        }
    })
}

/// Parses each line of `input` as `{key}: {value}` using the `each_line` callback
fn parse_pairs<'a, F>(input: &'a str, mut each_line: F) -> Result<(), Error<'a>>
    where F: FnMut(&'a str, &'a str) -> Result<(), Error<'a>>
{
    for line in input.lines() {
        let parts = &mut line.splitn(2, ": ");
        let k = try!(parts.next().ok_or(Error::MissingKey { line: line }));
        let v = try!(parts.next().ok_or(Error::MissingValue { line: line }));

        try!(each_line(k, v))
    }

    Ok(())
}

impl State {
    fn parse(input: &str) -> Result<Self, Error> {
        use State::*;

        Ok(match input {
            "play" => Play,
            "pause" => Pause,
            "stop" => Stop,
            _ => {
                return Err(Error::ParseType {
                    ty: "State",
                    value: input,
                })
            }
        })
    }
}

impl<'a> Song<'a> {
    /// Parses song information as outputted by `CurrentSong` and `PlaylistInfo`
    pub fn parse(input: &'a str) -> Result<Self, Error<'a>> {
        use self::Error::*;

        let expect = |k| {
            ExpectedKey {
                key: k,
                lines: input,
            }
        };

        let mut artist = Err(expect("Artist"));
        let mut title = Err(expect("Title"));

        try!(parse_pairs(input, |k, v| {
            match k {
                "Artist" => artist = Ok(v),
                "Title" => title = Ok(v),
                _ => {}
                // TODO uncomment
                // _ => return Err(UnhandledKeyValuePair { key: k, value: v }),
            }

            Ok(())
        }));

        Ok(Song {
            _0: (),
            artist: try!(artist),
            title: try!(title),
        })
    }
}

impl Time {
    fn parse(input: &str) -> Result<Time, Error> {
        use self::Error::*;

        let parts = &mut input.splitn(2, ':');

        let elapsed = try!(parts.next().ok_or(ParseType {
            ty: "Time",
            value: input,
        }));
        let total = try!(parts.next().ok_or(ParseType {
            ty: "Time",
            value: input,
        }));

        Ok(Time {
            _0: (),
            elapsed: try!(parse_ty!(elapsed, u32)),
            total: try!(parse_ty!(total, u32)),
        })
    }
}

impl Status {
    /// Parses the output of the `Status` command
    pub fn parse(input: &str) -> Result<Self, Error> {
        use self::Error::*;

        let expect = |k| {
            ExpectedKey {
                key: k,
                lines: input,
            }
        };

        let mut consume = Err(expect("consume"));
        let mut elapsed = None;
        let mut playlistlength = Err(expect("playlistlength"));
        let mut random = Err(expect("random"));
        let mut repeat = Err(expect("repeat"));
        let mut single = Err(expect("single"));
        let mut song = None;
        let mut state = Err(expect("state"));
        let mut time = None;
        let mut updating_db = None;
        let mut volume = Err(expect("volume"));

        try!(parse_pairs(input, |k, v| {
            match k {
                "consume" => consume = parse_bool(v),
                "elapsed" => elapsed = Some(try!(parse_ty!(v, f64))),
                "playlistlength" => playlistlength = parse_ty!(v, u32),
                "random" => random = parse_bool(v),
                "repeat" => repeat = parse_bool(v),
                "single" => single = parse_bool(v),
                "song" => song = Some(try!(parse_ty!(v, u32))),
                "state" => state = State::parse(v),
                "time" => time = Some(try!(Time::parse(v))),
                "updating_db" => updating_db = Some(try!(parse_ty!(v, u32))),
                "volume" => {
                    if v == "-1" {
                        volume = Ok(None)
                    } else {
                        volume = parse_ty!(v, u8).map(Some);
                    }
                }
                _ => {}
                // TODO uncomment
                // _ => return Err(UnhandledKeyValuePair { key: k, value: v }),
            }

            Ok(())
        }));

        let extra = song.map(|song| {
            Extra {
                _0: (),
                elapsed: elapsed,
                pos: song,
                time: time,
            }
        });

        Ok(Status {
            _0: (),
            consume: try!(consume),
            extra: extra,
            playlist_length: try!(playlistlength),
            random: try!(random),
            repeat: try!(repeat),
            single: try!(single),
            state: try!(state),
            updating_db: updating_db,
            volume: try!(volume),
        })
    }
}
