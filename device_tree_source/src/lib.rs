#[macro_use]
extern crate nom;

pub mod tree;
pub mod parser;

use std::str::{self, FromStr};
use std::path::{PathBuf, Path};
use std::fs::File;
use std::io::Read;
use std::cmp::Ordering;
use std::borrow::Borrow;

use nom::{IResult, ErrorKind, Needed, FindSubstring, digit, space, multispace, line_ending};

use parser::escape_c_string;

named!(pub parse_include<String>, preceded!(
    tag!("/include/"),
    preceded!( multispace,
        delimited!(
            char!('"'),
            escape_c_string,
            char!('"')
        ))
));

named!(pub find_include<(&[u8], String)>, do_parse!(
    pre: take_until!("/include/") >>
    path: parse_include >>
    (pre, path)
));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeBounds {
    pub path: PathBuf,
    pub global_start: usize,
    pub child_start: usize,
    pub len: usize,
    pub method: IncludeMethod,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncludeMethod {
    DTS,
    CPP,
}

impl PartialOrd for IncludeBounds {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IncludeBounds {
    fn cmp(&self, other: &Self) -> Ordering {
        use std::cmp::Ordering::*;
        match self.global_start.cmp(&other.global_start) {
            Equal => self.global_end().cmp(&other.global_end()),
            o => o,
        }
    }
}

impl IncludeBounds {
    pub fn global_end(&self) -> usize {
        self.global_start + self.len
    }

    pub fn split_bounds(bounds: &mut Vec<IncludeBounds>, start: usize, end: usize, offset: usize) {
        let mut remainders: Vec<IncludeBounds> = Vec::new();

        // println!("split s: {} e: {} off: {}", start, end, offset);

        for b in bounds.iter_mut() {
            // println!("g_start: {} g_end: {}", b.global_start, b.global_end());
            if b.global_start < start && b.global_end() >= start {
                // global_start -- start -- global_end
                let remainder = b.global_end() - start;

                // println!("remainder: {}", remainder);

                remainders.push(IncludeBounds {
                    path: b.path.clone(),
                    global_start: end,
                    child_start: b.child_start + start - b.global_start + offset,
                    len: remainder, // - offset,
                    method: b.method.clone(),
                });

                b.len = start - b.global_start;
            } else if b.global_start == start {
                // split is at begining of the bound
                // offset the start
                {
                    let offset = end - start;
                    b.global_start += offset;
                }
                // shrink the len by the offset
                b.len -= offset;
            }
        }

        bounds.extend_from_slice(&remainders);
        bounds.sort();
    }
}

#[derive(Debug, PartialEq)]
pub struct Linemarker {
    child_line: usize,
    path: PathBuf,
    flag: Option<LinemarkerFlag>,
}

#[derive(Debug, PartialEq)]
pub enum LinemarkerFlag {
    Start,
    Return,
    System,
    Extern,
}

named!(pub parse_linemarker<Linemarker>,
    complete!(do_parse!(
        tag!("#") >>
        opt!(tag!("line")) >>
        space >>
        line: map_res!(map_res!(digit, str::from_utf8), usize::from_str) >>
        space >>
        path: delimited!(
            char!('"'),
            map!(escape_c_string, PathBuf::from),
            char!('"')
        ) >>
        flag: opt!(preceded!(space, map_res!(map_res!(digit, str::from_utf8), u64::from_str))) >>
        line_ending >>
        (Linemarker {
            child_line: line,
            path: path,
            flag: flag.map(|f| match f {
                1 => LinemarkerFlag::Start,
                2 => LinemarkerFlag::Return,
                3 => LinemarkerFlag::System,
                4 => LinemarkerFlag::Extern,
                _ => unreachable!(),
            }),
        })
    ))
);

fn find_linemarker_start(input: &[u8]) -> IResult<&[u8], &[u8]> {
    if "# ".len() > input.len() {
        IResult::Incomplete(Needed::Size("# ".len()))
    } else {
        match input.find_substring("# ").iter().chain(input.find_substring("#line ").iter()).min() {
            None => {
                IResult::Error(error_position!(ErrorKind::TakeUntil, input))
            },
            Some(index) => {
                IResult::Done(&input[*index..], &input[0..*index])
            },
        }
    }
}

named!(find_linemarker<(&[u8], Linemarker)>, do_parse!(
    pre: find_linemarker_start >>
    marker: parse_linemarker >>
    (pre, marker)
));

fn parse_linemarkers(buf: &[u8], bounds: &mut Vec<IncludeBounds>, global_offset: usize) {
    let end_offset = global_offset + buf.len();
    // println!("{}", str::from_utf8(buf).unwrap());
    let mut buf = buf;
    println!("parsing linemarkers");
    loop {
        // look for linemarker
        if let IResult::Done(rem, (pre, marker)) = find_linemarker(buf) {
            // println!("{}", str::from_utf8(line).unwrap());
            // println!("{:?}", marker);
            // println!("pre.len() {}", pre.len());

            // double check that last bound was from a linemarker
            match bounds.last() {
                Some(&IncludeBounds { method: IncludeMethod::CPP, .. }) => {}
                _ => {
                    println!("{:#?}", bounds);
                    panic!("Linemarker found within DTS include")
                }
            }

            // end last
            bounds.last_mut().unwrap().len = pre.len();

            // start at new line
            let new_bound = IncludeBounds {
                path: marker.path.clone(),
                global_start: end_offset - rem.len(),
                child_start: File::open(&marker.path)
                                .map(|f| f.bytes().filter_map(|e| e.ok()))
                                .map(|b| line_to_byte_offset(b, marker.child_line).unwrap()) //TODO: unwraping is bad, SOK?
                                .unwrap_or(0),
                len: rem.len(),
                method: IncludeMethod::CPP,
            };

            bounds.push(new_bound);

            buf = rem;
        } else {
            return;
        }
    }    
}

pub fn include_files(path: &Path,
                     main_offset: usize)
                     -> Result<(Vec<u8>, Vec<IncludeBounds>), String> {
    // TODO: check from parent directory of root file
    let mut file = File::open(path).unwrap();
    let mut buffer: Vec<u8> = Vec::new();
    let mut bounds: Vec<IncludeBounds> = Vec::new();

    let mut string_buffer = String::new();
    file.read_to_string(&mut string_buffer).map_err(|_| "IO Error".to_string())?;

    let mut buf = string_buffer.as_bytes();

    named!(first_linemarker<(&[u8], Linemarker)>,
        do_parse!(
            marker: peek!(parse_linemarker) >>
            line: recognize!(parse_linemarker) >>
            (line, marker)
        )
    );

    let start_bound = if let IResult::Done(rem, (line, marker)) = first_linemarker(buf) {
        let bound = IncludeBounds {
            path: marker.path.clone(),
            global_start: buf.len() - rem.len(),
            // TODO: check from parent directory of root file
            child_start: File::open(&marker.path)
                             .map(|f| f.bytes().filter_map(|e| e.ok()))
                             .map(|b| line_to_byte_offset(b, marker.child_line).unwrap()) //TODO: unwraping is bad, SOK?
                             .unwrap_or(0),
            len: File::open(&marker.path).unwrap().bytes().count(),
            method: IncludeMethod::CPP,
        };

        buffer.extend_from_slice(line);
        buf = rem;

        bound
    } else {
        // println!("main_offset {}", main_offset);
        IncludeBounds {
            path: path.to_path_buf(),
            global_start: main_offset,
            child_start: 0,
            // TODO: check from parent directory of root file
            len: File::open(path).unwrap().bytes().count(),
            method: IncludeMethod::DTS,
        }
    };
    bounds.push(start_bound);

    loop {
        // go until /include/
        if let IResult::Done(rem, (pre, file)) = find_include(&buf[..]) {
            parse_linemarkers(pre, &mut bounds, buffer.len());
            buffer.extend_from_slice(pre);

            let offset = pre.len();
            // println!("{}", file);
            // println!("Offset: {}", offset);
            // println!("{}", include_tree);

            let included_path = Path::new(&file);
            let total_len = buffer.len() + main_offset; // - 1;
            let (sub_buf, sub_bounds) = include_files(included_path, total_len)?;
            buffer.extend(sub_buf);

            let inc_start = sub_bounds.first()
                                      .map(|b| b.global_start)
                                      .expect(&format!("No bounds returned: {}",
                                                      included_path.to_string_lossy()));
            let inc_end = sub_bounds.last()
                                    .map(|b| b.global_end())
                                    .expect(&format!("No bounds returned: {}",
                                                    included_path.to_string_lossy()));
            let eaten_len = (buf.len() - offset) - rem.len();
            //include_tree.offset_after_location(inc_start,
            //                                   inc_end as isize - inc_start as isize -
            //                                   eaten_len as isize);
            // println!("After offset");
            // println!("{}", include_tree);
            IncludeBounds::split_bounds(&mut bounds, inc_start, inc_end, eaten_len);
            bounds.extend_from_slice(&sub_bounds);
            bounds.sort();

            // println!("After split");
            // println!("{}", include_tree);

            buf = rem;
        } else {
            parse_linemarkers(buf, &mut bounds, buffer.len());
            // no more includes, just add the rest and return
            buffer.extend(buf);
            return Ok((buffer, bounds));
        }
    }
}

pub fn line_to_byte_offset<K, I>(bytes: I, line: usize) -> Result<usize, String>
    where K: Borrow<u8> + Eq,
          I: Iterator<Item = K>
{
    if line == 1 {
        Ok(0)
    } else {
        bytes.enumerate()
            .filter(|&(_, ref byte)| byte.borrow() == &b'\n')
            .nth(line - 2)
            .map(|(offset, _)| offset + 1)
            .ok_or_else(|| "Failed converting from line to byte offset".to_string())
    }
}

pub fn byte_offset_to_line_col<K, I>(bytes: I, offset: usize) -> (usize, usize)
    where K: Borrow<u8> + Eq,
          I: Iterator<Item = K>
{
    let opt = bytes.enumerate()
        .filter(|&(off, ref byte)| off < offset && byte.borrow() == &b'\n')
        .map(|(start, _)| start)
        .enumerate()
        .last();

    match opt {
        Some((line, start)) => (line + 2, offset - start),
        None => (1, offset + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nom::IResult;

    #[test]
    fn lines_to_bytes() {
        let string = "Howdy\nHow goes it\n\nI'm doing fine\n";
        assert_eq!(line_to_byte_offset(string.as_bytes().iter(), 1).unwrap(),
                   0);
        assert_eq!(line_to_byte_offset(string.as_bytes().iter(), 2).unwrap(),
                   6);
        assert_eq!(line_to_byte_offset(string.as_bytes().iter(), 3).unwrap(),
                   18);
        assert_eq!(line_to_byte_offset(string.as_bytes().iter(), 4).unwrap(),
                   19);
    }

    #[test]
    fn bytes_to_lines() {
        let string = "Howdy\nHow goes it\n\nI'm doing fine\n";
        assert_eq!(byte_offset_to_line_col(string.as_bytes().iter(), 0),
                   (1, 1));
        assert_eq!(byte_offset_to_line_col(string.as_bytes().iter(), 8),
                   (2, 3));
        assert_eq!(byte_offset_to_line_col(string.as_bytes().iter(), 20),
                   (4, 2));
        assert_eq!(byte_offset_to_line_col(string.as_bytes().iter(), 18),
                   (3, 1));
    }

    #[test]
    fn linemarker_no_flag() {
        let input = b"# 1 \"<built-in>\"\n";
        assert_eq!(
            parse_linemarker(input),
            IResult::Done(
                &b""[..],
                Linemarker {
                    child_line: 1,
                    path: PathBuf::from("<built-in>"),
                    flag: None,
                }
            )
        );
    }

    #[test]
    fn linemarker_flag() {
        let input = b"# 12 \"am33xx.dtsi\" 2\n";
        assert_eq!(
            parse_linemarker(input),
            IResult::Done(
                &b""[..],
                Linemarker {
                    child_line: 12,
                    path: PathBuf::from("am33xx.dtsi"),
                    flag: Some(LinemarkerFlag::Return),
                }
            )
        );
    }
}
