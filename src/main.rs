use std::thread;
use std::time::{self, Instant};
use std::io::{Stdout, Write};
use std::process::{exit};
use termion::AsyncReader;
use termion::event::Key::{Char, Ctrl};
use termion::input::TermRead;
use termion::raw::{IntoRawMode, RawTerminal};

extern crate termion;

struct Buffer {
    pub lines: Vec<String>,
    pub cursor: (u16, u16),
}

impl Buffer {
    fn new() -> Buffer {
        Buffer{
            lines: vec![],
            cursor: (1, 1),
        }
    }

    fn add_line(&mut self, line: String) {
        self.lines.push(line);
    }
}

struct Editor {
    pub buffers: Vec<Buffer>,
    pub current_buffer: usize,
    pub stdout: RawTerminal<Stdout>,
    pub stdin: termion::input::Events<AsyncReader>,
}

impl Editor {
    fn new() -> Editor {
        let mut b = Buffer::new();
        b.add_line(String::from("Hello World 1"));
        b.add_line(String::from("Hello World 2"));

        let stdout = std::io::stdout().into_raw_mode().unwrap();
        let stdin = termion::async_stdin().events();

        Editor {
            buffers: vec![b],
            current_buffer: 0,
            stdout: stdout,
            stdin: stdin,
        }
    }

    fn render(&mut self) -> std::io::Result<()> {
        write!(self.stdout, "{}{}{}", termion::clear::All, termion::cursor::Hide, termion::cursor::Goto(1, 1))?;

        let num_lines = self.buffers[self.current_buffer].lines.len();
        for i in 0..num_lines {
            if i < num_lines - 1 {
                write!(self.stdout, "{}\r\n", self.buffers[self.current_buffer].lines[i])?;
            } else {
                write!(self.stdout, "{}", self.buffers[self.current_buffer].lines[i])?;
            }
        }

        let cursor = self.buffers[self.current_buffer].cursor;
        write!(self.stdout, "{}{}", termion::cursor::Goto(cursor.0, cursor.1), termion::cursor::Show)?;

        self.stdout.flush()?;

        Ok(())
    }

    fn reset_stdout(&mut self) -> std::io::Result<()> {
        self.stdout.suspend_raw_mode()?;
        write!(self.stdout, "{}", termion::cursor::Show)?;
        Ok(())
    }

    fn handle_input(&mut self) -> std::io::Result<()> {
        let event = self.stdin.next();
        match event {
            Some(Ok(termion::event::Event::Key(k))) => {
                match k {
                    Ctrl('q') => {
                        self.reset_stdout()?;
                        exit(0)
                    },
                    Char(_) => {},
                    _ => {},
                };
            },
            _ => {},
        };
        Ok(())
    }

    pub fn run(&mut self) -> std::io::Result<()> {
        loop {
            let start_time = Instant::now();
            self.render()?;
            self.handle_input()?;
            let end_time = start_time.elapsed();
            if end_time.as_millis() < 33 {
                thread::sleep(time::Duration::from_millis(33 - end_time.as_millis() as u64));
            }
        }
    }
}

fn main() -> std::io::Result<()> {
    let mut e = Editor::new();
    e.run()?;

    Ok(())
}
