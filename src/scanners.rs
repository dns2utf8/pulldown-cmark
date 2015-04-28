// Copyright 2015 Google Inc. All rights reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Scanners for fragments of CommonMark syntax

use entities;
use utils;
use std::borrow::Cow;
use std::borrow::Cow::{Borrowed, Owned};
use std::char;

// sorted for binary_search
const ASCII_PUNCTUATION: &'static [u8] = b"!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

// sorted for binary_search
const HTML_TAGS: [&'static str; 50] = ["article", "aside", "blockquote",
	"body", "button", "canvas", "caption", "col", "colgroup", "dd", "div",
	"dl", "dt", "embed", "fieldset", "figcaption", "figure", "footer", "form",
	"h1", "h2", "h3", "h4", "h5", "h6", "header", "hgroup", "hr", "iframe",
	"li", "map", "object", "ol", "output", "p", "pre", "progress", "script",
	"section", "style", "table", "tbody", "td", "textarea", "tfoot", "th",
	"thead", "tr", "ul", "video"];

pub fn is_ascii_whitespace(c: u8) -> bool {
	(c >= 0x09 && c <= 0x0d) || c == b' '
}

pub fn is_ascii_whitespace_no_nl(c: u8) -> bool {
	c == b'\t' || (c >= 0x0b && c <= 0x0d) || c == b' '
}

pub fn is_ascii_alphanumeric(c: u8) -> bool {
	match c {
		b'0' ... b'9' | b'a' ... b'z' | b'A' ... b'Z' => true,
		_ => false
	}
}

fn is_hexdigit(c: u8) -> bool {
	match c {
		b'0' ... b'9' | b'a' ... b'f' | b'A' ... b'F' => true,
		_ => false
	}
}

fn is_digit(c: u8) -> bool {
	b'0' <= c && c <= b'9'
}

pub fn is_ascii_punctuation(c: u8) -> bool {
	ASCII_PUNCTUATION.binary_search(&c).is_ok()
}

// scan a single character
pub fn scan_ch(data: &str, c: u8) -> usize {
	if !data.is_empty() && data.as_bytes()[0] == c { 1 } else { 0 }
}

pub fn scan_while<F>(data: &str, f: F) -> usize 
	where F: Fn(u8) -> bool {
	let mut i = 0;
	while i < data.len() && f(data.as_bytes()[i]) {
		i += 1;
	}
	i	
}

pub fn scan_ch_repeat(data: &str, c: u8) -> usize {
	scan_while(data, |x| x == c)
}

// TODO: maybe should scan unicode whitespace too
pub fn scan_whitespace_no_nl(data: &str) -> usize {
	scan_while(data, is_ascii_whitespace_no_nl)
}

// Maybe returning Option<usize> would be more Rustic?
pub fn scan_eol(s: &str) -> (usize, bool) {
	if s.is_empty() { return (0, true); }
	match s.as_bytes()[0] {
		b'\n' => (1, true),
		b'\r' => (if s[1..].starts_with('\n') { 2 } else { 1 }, true),
		_ => (0, false)
	}
}

// Maybe Option, size of 0 makes sense at EOF
pub fn scan_blank_line(text: &str) -> usize {
	let i = scan_whitespace_no_nl(text);
	if let (n, true) = scan_eol(&text[i..]) {
		i + n
	} else {
		0
	}
}

pub fn scan_nextline(s: &str) -> usize {
	match s.find('\n') {
		Some(x) => x + 1,
		None => s.len()
	}
}

// returned pair is (number of bytes, number of spaces)
pub fn calc_indent(text: &str, max: usize) -> (usize, usize) {
	let mut i = 0;
	let mut spaces = 0;
	while i < text.len() && spaces < max {
		match text.as_bytes()[i] {
			b' ' => spaces += 1,
			b'\t' => {
				let new_spaces = spaces + 4 - (spaces & 3);
				if new_spaces > max {
					break;
				}
				spaces = new_spaces;
			},
			_ => break
		}
		i += 1;
	}
	(i, spaces)
}

// return size of line containing hrule, including trailing newline, or 0
pub fn scan_hrule(data: &str) -> usize {
	let size = data.len();
	let mut i = calc_indent(data, 3).0;
	if i + 2 >= size { return 0; }
	let c = data.as_bytes()[i];
	if !(c == b'*' || c == b'-' || c == b'_') { return 0; }
	let mut n = 0;
	while i < size {
		match data.as_bytes()[i] {
			b'\n' => {
				i += 1;
				break;
			}
			c2 if c2 == c => n += 1,
			b' ' => (),
			_ => return 0
		}
		i += 1;
	}
	if n >= 3 { i } else { 0 }
}

// returns number of bytes in prefix and level
pub fn scan_atx_header(data: &str) -> (usize, i32) {
	let size = data.len();
	let (start, _) = calc_indent(data, 3);
	let level = scan_ch_repeat(&data[start..], b'#');
	let i = start + level;
	if level >= 1 && level <= 6 {
		if i < size {
			match data.as_bytes()[i] {
				b' ' | b'\t' ... b'\r' => (),
				_ => return (0, 0)
			}
		}
		(i, level as i32)
	} else {
		(0, 0)
	}
}

// returns number of bytes in line (including trailing newline) and level
pub fn scan_setext_header(data: &str) -> (usize, i32) {
	let size = data.len();
	let (mut i, _) = calc_indent(data, 3);
	if i == size { return (0, 0); }
	let c = data.as_bytes()[i];
	if !(c == b'-' || c == b'=') { return (0, 0); }
	i += 1 + scan_ch_repeat(&data[i + 1 ..], c);
	let n = scan_blank_line(&data[i..]);
	if n == 0 { return (0, 0); }
	i += n;
	let level = if c == b'=' { 1 } else { 2 };
	(i, level)
}

// returns: number of bytes scanned, char, count, indent
// Note: somewhat redundant, as bytes scanned = count + indent
pub fn scan_code_fence(data: &str) -> (usize, u8, usize, usize) {
	let (beg, _) = calc_indent(data, 3);
	if beg == data.len() { return (0, 0, 0, 0); }
	let c = data.as_bytes()[beg];
	if !(c == b'`' || c == b'~') { return (0, 0, 0, 0); }
	let i = beg + 1 + scan_ch_repeat(&data[beg + 1 ..], c);
	if (i - beg) >= 3 {
		if c == b'`' {
			let next_line = i + scan_nextline(&data[i..]);
			if data[i..next_line].find('`').is_some() {
				return (0, 0, 0, 0);
			}
		}
		return (i, c, i - beg, beg);
	}
	(0, 0, 0, 0)
}

pub fn scan_backticks(data: &str) -> usize {
	scan_ch_repeat(data, b'`')
}

pub fn scan_blockquote_start(data: &str) -> usize {
	let n = calc_indent(data, 3).0;
	if data[n..].starts_with('>') {
		let n = n + 1;
		n + scan_ch(&data[n..], b' ')
	} else {
		0
	}
}

// return number of bytes scanned, delimeter, start index, and indent
pub fn scan_listitem(data: &str) -> (usize, u8, usize, usize) {
	let (n, indent) = calc_indent(data, 3);
	if n == data.len() { return (0, 0, 0, 0); }
	let mut c = data.as_bytes()[n];
	let mut start = 0;
	let w = match c {
		b'-' | b'+' | b'*' => 1,
		b'0' ... b'9' => {
			let mut i = n + 1;
			i += scan_while(&data[i..], is_digit);
			start = data[n..i].parse().unwrap();
			if i >= data.len() { return (0, 0, 0, 0); }
			c = data.as_bytes()[i];
			if !(c == b'.' || c == b')') { return (0, 0, 0, 0); }
			i + 1 - n
		}
		_ => { return (0, 0, 0, 0); }
	};
	let (mut postn, mut postindent) = calc_indent(&data[n + w .. ], 5);
	if postindent == 0 {
		if !scan_eol(&data[n + w ..]).1 { return (0, 0, 0, 0); }
		postindent += 1;
	} else if postindent > 4 {
		postn = 1;
		postindent = 1;
	}
	(n + w + postn, c, start, indent + w + postindent)
}

// return whether delimeter run can open or close
pub fn compute_open_close(data: &str, loc: usize, c: u8) -> (usize, bool, bool) {
	// TODO: handle Unicode, not just ASCII
	let size = data.len();
	let mut end = loc + 1;
	while end < size && data.as_bytes()[end] == c {
		end += 1;
	}
	let mut beg = loc;
	while beg > 0 && data.as_bytes()[beg - 1] == c {
		beg -= 1;
	}
	let (white_before, punc_before) = if beg == 0 {
		(true, false)
	} else {
		let c = data.as_bytes()[beg - 1];
		(is_ascii_whitespace(c), is_ascii_punctuation(c))
	};
	let (white_after, punc_after) = if end == size {
		(true, false)
	} else {
		let c = data.as_bytes()[end];
		(is_ascii_whitespace(c), is_ascii_punctuation(c))
	};
	let left_flanking = !white_after && (!punc_after || white_before || punc_before);
	let right_flanking = !white_before && (!punc_before || white_after || punc_after);
	let (can_open, can_close) = match c {
		b'*' => (left_flanking, right_flanking),
		b'_' => (left_flanking && !right_flanking, right_flanking && !left_flanking),
		_ => (false, false)
	};
	(end - loc, can_open, can_close)
}

fn cow_from_codepoint_str(s: &str, radix: u32) -> Cow<'static, str> {
	let codepoint = u32::from_str_radix(s, radix).unwrap();
	Owned(char::from_u32(codepoint).unwrap_or('\u{FFFD}').to_string())
}

// doesn't bother to check data[0] == '&'
pub fn scan_entity(data: &str) -> (usize, Option<Cow<'static, str>>) {
	let size = data.len();
	let mut end = 1;
	if scan_ch(&data[end..], b'#') == 1 {
		end += 1;
		if end < size && (data.as_bytes()[end] == b'x' || data.as_bytes()[end] == b'X') {
			end += 1;
			end += scan_while(&data[end..], is_hexdigit);
			if end > 3 && end < 12 && scan_ch(&data[end..], b';') == 1 {
				return (end + 1, Some(cow_from_codepoint_str(&data[3..end], 16)));
			}
		} else {
			end += scan_while(&data[end..], is_digit);
			if end > 2 && end < 11 && scan_ch(&data[end..], b';') == 1 {
				return (end + 1, Some(cow_from_codepoint_str(&data[2..end], 10)));
			}
		}
		return (0, None);
	}
	end += scan_while(&data[end..], is_ascii_alphanumeric);
	if scan_ch(&data[end..], b';') == 1 {
		if let Some(value) = entities::get_entity(&data[1..end]) {
			return (end + 1, Some(Borrowed(value)));
		}
	}
	return (0, None);
}

pub fn is_escaped(data: &str, loc: usize) -> bool {
	let mut i = loc;
	while i >= 1 && data.as_bytes()[i - 1] == b'\\' {
		i -= 1;
	}
	((loc - i) & 1) != 0
}

// Remove backslash escapes and resolve entities
pub fn unescape<'a>(input: &'a str) -> Cow<'a, str> {
	if input.find(|c| c == '\\' || c == '&').is_none() {
		Borrowed(input)
	} else {
		let mut result = String::new();
		let mut mark = 0;
		let mut i = 0;
		while i < input.len() {
			match input.as_bytes()[i] {
				b'\\' => {
					result.push_str(&input[mark..i]);
					i += 1;
					mark = i;
				}
				b'&' => {
					match scan_entity(&input[i..]) {
						(n, Some(value)) => {
							result.push_str(&input[mark..i]);
							result.push_str(&value);
							i += n;
							mark = i;
						}
						_ => i += 1
					}
				}
				_ => i += 1
			}
		}
		result.push_str(&input[mark..]);
		Owned(result)
	}
}

pub fn is_html_tag(tag: &str) -> bool {
	HTML_TAGS.binary_search_by(|probe| utils::strcasecmp(probe, tag)).is_ok()
}
