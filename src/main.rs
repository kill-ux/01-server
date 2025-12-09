// // use mio::{Events, Interest, Poll, net::TcpListener};
// // use server_proxy::config::*;
// use server_proxy::error::Result;
// // use server_proxy::server::Token;
// use std::{io::{Read, Write}, net::{SocketAddr, TcpListener, TcpStream}};

// const BUFFER_SIZE: usize = 512;
// const ADDRESS: &str = "127.0.0.1:13265";

// fn handle_client(mut stream: TcpStream) {
//     println!("New connection from : {}", stream.peer_addr().unwrap());
//     let mut buffer = [0; BUFFER_SIZE];

//     loop {
//         match stream.read(&mut buffer) {
//             Ok(0) => {
//                 println!("Client disconnected");
//                 break;
//             }
//             Ok(bytes_read) => {
//                 let data = &buffer[..bytes_read];
//                 println!("Received {} bytes: {:?}", bytes_read, data);

//                 if stream.write_all(data).is_err() {
//                     println!("Failed to flush stream.");
//                     break;
//                 }
//             }
//             Err(e) => {
//                 eprintln!("An error occurred: {}", e);
//                 break;
//             }
//         }
//     }
// }

// fn main() -> Result<()> {
//     // let config =  Config::parse()?;
//     // let tokens: Token = Token::new();
//     // let mut poll = Poll::new()?;
//     // let mut events = Events::with_capacity(5);

//     // for server in &config.servers {
//     //     for port in &server.ports {
//     //         let addr: SocketAddr  =   format!("{}:{}",server.host, port).parse()?;
//     //         let mut listner = TcpListener::bind(addr)?;
//     //         poll.registry().register(&mut listner,mio::Token(tokens.next()) , Interest::READABLE)?;
//     //         dbg!(&addr);
//     //     }
//     // }

//     // loop {
//     //     poll.poll(&mut events, None)?;

//     //     for event in events.iter() {
//     //         match event.token() {

//     //         }
//     //     }
//     // }

//     let listner = TcpListener::bind(ADDRESS)?;
//     for con in listner.incoming() {
//         match con {
//             Ok(stream) => {
//                 dbg!(&stream);
//                 handle_client(stream);
//             }
//             Err(err) => {
//                 println!("{}",err)
//             }
//         }
//     }
//     Ok(())
// }

use std::{
    pin::Pin,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
    thread,
};

const VTABLE: RawWakerVTable = RawWakerVTable::new(clone_waker, wake, wake_by_ref, drop_waker);

unsafe fn clone_waker(data: *const ()) -> RawWaker {
    let arc =
        Arc::from_raw(data as *const (Arc<Mutex<Vec<Pin<Box<dyn Future<Output = ()>>>>>>, Condvar));
    let new_arc = Arc::clone(&arc);
    let _ = Arc::into_raw(arc);
    RawWaker::new(Arc::into_raw(new_arc) as *const (), &VTABLE)
}

unsafe fn wake(data: *const ()) {
    let arc =
        Arc::from_raw(data as *const (Arc<Mutex<Vec<Pin<Box<dyn Future<Output = ()>>>>>>, Condvar));
    arc.1.notify_one();
}

unsafe fn wake_by_ref(data: *const ()) {
    wake(data);
}

unsafe fn drop_waker(data: *const ()) {
    let _ =
        Arc::from_raw(data as *const (Arc<Mutex<Vec<Pin<Box<dyn Future<Output = ()>>>>>>, Condvar));
}

struct DelayedPrinter {
    remaining_polls: usize,
    message: String,
}

impl Future for DelayedPrinter {
    type Output = ();

    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if (self.remaining_polls == 0) {
            println!("✅ Future finished: {}", self.message);
            Poll::Ready(())
        } else {
            let waker = cx.waker().clone();
            println!(
                "⏳ Future pending: {}. Remaining polls: {}",
                self.message, self.remaining_polls
            );
            let remaining_polls = self.remaining_polls;
            thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(50));
                println!(
                    "    ⏰ Waking up task for polls: {} (Simulated I/O Ready)",
                    remaining_polls
                );
                waker.wake();
            });

            self.remaining_polls -= 1;
            Poll::Pending
        }
    }
}

struct Executor {
    task_queue: Arc<Mutex<Vec<Pin<Box<dyn Future<Output = ()>>>>>>,
    condvar: Arc<Condvar>,
}

impl Executor {
    fn new() -> Self {
        Executor {
            task_queue: Arc::new(Mutex::new(Vec::new())),
            condvar: Arc::new(Condvar::new()),
        }
    }

    fn spawn(&self, future: impl Future<Output = ()> + 'static) {
        let mut queue = self.task_queue.lock().unwrap();
        queue.push(Box::pin(future));
    }

    fn run(&self) {
        let queue_clone = self.task_queue.clone();
        let condvar_clone = self.condvar.clone();

        let raw_waker = RawWaker::new(
            Arc::into_raw(Arc::new((queue_clone, condvar_clone))) as *const (),
            &VTABLE,
        );
        let waker = unsafe { Waker::from_raw(raw_waker) };

        let mut context = Context::from_waker(&waker);

        loop {
            let mut queue = self.task_queue.lock().unwrap();

            if queue.is_empty() {
                println!("\n😴 Executor blocked (No ready tasks). Waiting for wake...");
                queue = self.condvar.wait(queue).unwrap();
            }

            let mut pending_tasks = Vec::new();

            while let Some(mut task) = queue.pop() {
                match task.as_mut().poll(&mut context) {
                    Poll::Ready(_) => {

                    }
                    Poll::Pending => {
                        pending_tasks.push(task);
                    }
                }
            }

            queue.append(&mut pending_tasks);

            if queue.is_empty()
        }
    }
}

fn main() {
    println!("Hello World");
    let executor = Executor::new();

    executor.spawn(DelayedPrinter {
        remaining_polls: 3,
        message: "Task A".to_string(),
    });
    executor.spawn(DelayedPrinter {
        remaining_polls: 1,
        message: "Task B".to_string(),
    });
    executor.spawn(DelayedPrinter {
        remaining_polls: 2,
        message: "Task C".to_string(),
    });

    executor.run()
}
