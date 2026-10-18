fn main() {
    let args: Vec<String> = std::env::args().collect();
    let target = args.get(1).map(String::as_str).unwrap_or("html");
    let file = args.get(2).expect("usage: replay <target> <file>");
    let data = std::fs::read(file).unwrap();
    match target {
        "html" => dillo_fuzz::html(&data),
        "css" => dillo_fuzz::css(&data),
        "decode" => dillo_fuzz::decode(&data),
        "dpip" => dillo_fuzz::dpip(&data),
        "files" => dillo_fuzz::files(&data),
        "url" => dillo_fuzz::url(&data),
        "images" => dillo_fuzz::images(&data),
        _ => eprintln!("unknown target"),
    }
    println!("no panic");
}
