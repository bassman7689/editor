extern crate termion;

fn main() {
    print!("{}{}Stuff", termion::clear::All, termion::cursor::Goto(1, 1));
}
