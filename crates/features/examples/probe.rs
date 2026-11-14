use saphyr_parser::{Event, Parser};
fn main() {
    let inputs = [
        "name: demo\ndescription: Does a thing.",
        "name: \"demo\" # trailing\ndescription: >\n  Folded\n  text: with a colon.\n",
        "name: demo\n  description: nested\nlicense: x",
        "name: demo\nmetadata:\n\tauthor: me",
        "name: one\ndescription: x\nname: two",
        "name: [unclosed\nlicense: MIT",
        "description: &d old\nz: *d",
        "a: &a [x, x]\nb: &b [*a, *a]",
        "name: n\ndescription: |\n  old\n\n\nz: 1",
        "- a\n- b",
        "just text",
        "",
        "# only\n",
        "name:\ndescription: [a]",
        "name: !tag foo",
    ];
    for input in inputs {
        println!("=== {input:?}");
        let parser = Parser::new_from_str(input);
        for item in parser {
            match item {
                Ok((event, span)) => println!(
                    "{:?} [{}:{}:{} .. {}:{}:{}]",
                    event,
                    span.start.index(),
                    span.start.line(),
                    span.start.col(),
                    span.end.index(),
                    span.end.line(),
                    span.end.col()
                ),
                Err(e) => {
                    println!("ERR {e:?} {}", e);
                    break;
                }
            }
        }
    }
}
