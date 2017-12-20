extern crate regex;

use std::net::{TcpListener, TcpStream};
use std::io::prelude::*;
use std::fs::File;

use std::thread;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;

use regex::Regex;

use RouteAction::*;

enum RouteAction {
    Open(String),
    None
}

struct Route {
    pattern: Regex,
    action: RouteAction
}

enum HttpResult {
    HttpOk(String),
    HttpNotFound(String),
    HttpServerError(String)
}

use HttpResult::*;

type Job = Box<FnBox + Send + 'static>;

struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>
}

enum Message {
    NewJob(Job),
    Terminate
}

use Message::*;

pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: mpsc::Sender<Message>
}

impl ThreadPool {
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        let (sender, receiver) = mpsc::channel();

        let mut workers = Vec::with_capacity(size);

        let receiver = Arc::new(Mutex::new(receiver));

        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }

        ThreadPool { workers, sender }
    }

    pub fn execute<F>(&self, f: F)
        where
        F: FnOnce() + Send + 'static
    {
        let job = Box::new(f);

        self.sender.send(NewJob(job)).unwrap();
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        for _ in &mut self.workers {
            self.sender.send(Terminate).unwrap();
        }

        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                thread.join().unwrap();
            }
        }
    }
}

trait FnBox {
    fn call_box(self: Box<Self>);
}

impl<F: FnOnce()> FnBox for F {
    fn call_box(self: Box<F>) {
        (*self)()
    }
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Message>>>) -> Worker {
        let thread = thread::spawn(move || {
            loop {
                let message = receiver.lock().unwrap().recv().unwrap();


                match message {
                    NewJob(job) => {
                        println!("Got job for worker {}, executing...", id);
                        job.call_box();
                    },
                    Terminate => break,
                };
            }
        });

        Worker {
            id,
            thread: Some(thread),
        }
    }
}


fn main() {

    let mut routes = vec!();

    let mut route_file = File::open("routes.txt").expect("Failed to open routes.txt file!");

    let mut route_file_contents = String::new();
    route_file.read_to_string(&mut route_file_contents).expect("Failed to read routes.txt file");

    let route_p =
        Regex::new("(?im)^\\s*\"(?P<route>[^\"]+)\"\\s+:\\s+(?P<type>\\w+)\\s*\"(?P<arg>[^\"]+)\"$").unwrap();

    for capture in route_p.captures_iter(route_file_contents.as_ref()) {
        // println!("Route: {}, Type: {}, Arg: {}", &capture["route"], &capture["type"], &capture["arg"]);

        let route = Regex::new(&capture["route"]).expect(String::from(format!("Failed to build regex for route '{}', cannot start!", &capture["route"])).as_ref());

        let typ = &capture["type"];
        let arg = &capture["arg"];

        let routeaction : RouteAction = match typ {
            "open" => Open(String::from(arg)),
            &_ => None
        };

        routes.push(Route { pattern: route, action: routeaction });
    }
    
    let listener = TcpListener::bind("127.0.0.1:8080").unwrap();

    let pool = ThreadPool::new(4);

    let routes = Arc::new(routes);

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        let routes = Arc::clone(&routes);

        pool.execute(move || {
            handle_connection(stream, &routes);
        });
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
        HttpOk(response) => format!("HTTP/1.1 200 Ok\r\n\r\n{}", response),
        HttpNotFound(message) => format!("HTTP/1.1 404 Not Found\r\n\r\n{}", message),
        HttpServerError(error) => format!("HTTP/1.1 500 Server Error\r\n\r\n{}", error)
    }
}

fn match_route<'a>(route_to_match: &str, routes: &'a Vec<Route>) -> Option<&'a Route> {
    for route in routes {
        if route.pattern.is_match(route_to_match) {
            return Some(route)
        }
    }
    return Option::None
}

fn handle_connection(mut stream: TcpStream, routes: &Vec<Route>) {

    let get_p = Regex::new(r"^GET (?P<route>[/a-zA-Z0-9.]+) HTTP/1\.1.*").unwrap();

    let mut buffer = [0; 512];
    stream.read(&mut buffer).unwrap();

    let request = String::from_utf8_lossy(&buffer[..]);

    println!("Request {}", &request);

    for cap in get_p.captures_iter(&request) {
        let route_to_get = &cap["route"];
        
        let route = match_route(route_to_get, &routes);

        let response = match route {
            Some(route) => {
                match route.action {
                    Open(ref path) => {
                        println!("Found match for route {} serving file {}", &route.pattern, path);
                        serve_file(path.as_ref())
                    },
                    None => panic!("I don't know what to do with this route!!!"),
                }
            },
            Option::None => HttpNotFound(format!("Route at {}, not found!", &route_to_get))
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
