extern crate regex;

use std::net::{TcpListener, TcpStream};
use std::io::prelude::*;
use std::fs::File;
use std::collections::HashMap;

use regex::Regex;

use RouteAction::*;

enum RouteAction {
    Open(String),
    None
}

enum HttpResult {
    HttpOk(String),
    HttpNotFound(String),
    HttpServerError(String)
}

use HttpResult::*;

fn main() {

    let mut routes = HashMap::new();

    let mut route_file = File::open("routes.txt").expect("Failed to open routes.txt file!");

    let mut route_file_contents = String::new();
    route_file.read_to_string(&mut route_file_contents).expect("Failed to read routes.txt file");

    let route_p =
        Regex::new("(?im)^\\s*\"(?P<route>[^\"]+)\"\\s+:\\s+(?P<type>\\w+)\\s*\"(?P<arg>[^\"]+)\"$").unwrap();

    for capture in route_p.captures_iter(route_file_contents.as_ref()) {
        // println!("Route: {}, Type: {}, Arg: {}", &capture["route"], &capture["type"], &capture["arg"]);

        let route = String::from(&capture["route"]);
        let typ = &capture["type"];
        let arg = &capture["arg"];

        let routeaction : RouteAction = match typ {
            "open" => Open(String::from(arg)),
            &_ => None
        };

        routes.insert(route, routeaction);
    }
    
    let listener = TcpListener::bind("127.0.0.1:8080").unwrap();

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        handle_connection(stream, &routes);
        println!("Connection established!");
    }
}

fn serve_file(path: &str) -> HttpResult {
    let file = File::open(path);

    match file {
        Ok(mut file) => {
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();

            HttpOk(contents)
        },
        Err(_) => {
            HttpServerError(format!("File {} not found!", &path))
        }
    }
}

fn render_result(result: HttpResult) -> String {
    match result {
        HttpOk(response) => format!("HTTP/1.1 200 Ok\n\n{}", response),
        HttpNotFound(message) => format!("HTTP/1.1 404 Not Found\n\n{}", message),
        HttpServerError(error) => format!("HTTP/1.1 500 Server Error\n\n{}", error)
    }
}

fn handle_connection(mut stream: TcpStream, routes: &HashMap<String, RouteAction>) {

    let get_p = Regex::new(r"^GET (?P<route>[/a-zA-Z0-9.]+) HTTP/1\.1.*").unwrap();

    let mut buffer = [0; 512];
    stream.read(&mut buffer).unwrap();

    let request = String::from_utf8_lossy(&buffer[..]);

    println!("Request {}", &request);

    for cap in get_p.captures_iter(&request) {
        let route = &cap["route"];
        
        let routeaction = routes.get(route);

        let response = match routeaction {
            Some(&Open(ref path)) => serve_file(path.as_ref()),
            Some(&None) => panic!("I don't know what to do with this route!!!"),
            Option::None => HttpNotFound(format!("Route at {}, not found!", &route))
        };

        let _response = render_result(response);

        if let Err(error) = stream
            .write(_response.as_bytes())
            .and_then(|_| {
                stream.flush()
            }) {
                println!("Failed to write to stream: {}", error);
            }
    }

}
