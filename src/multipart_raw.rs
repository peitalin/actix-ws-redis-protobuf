
use actix_web::{
    dev,
    error,
    error::MultipartError,
    http,
    middleware,
    middleware::cors::Cors,
    multipart,
    server,
    ws,
    App, AsyncResponder,
    Error,
    Either,
    FutureResponse,
    HttpRequest, HttpResponse, HttpMessage,
    Json,
};
use bytes::{Buf, IntoBuf, BytesMut};
use futures::future::{Future, join_all};
use futures::{future, stream, Stream};

use std::sync::Arc;
use std::io::Read;
use std::io::Write;

use crate::AppState;


pub fn save_file(
    file_path: String,
    field: multipart::Field<dev::Payload>,
) -> Box<Future<Item = (String, i64), Error = Error>> {

    let mut stdin = match std::process::Command::new("gsutil")
        .arg("-h")
        .arg(format!("Content-Type:{}", &field.content_type()))
        .arg("cp")
        .arg("-")
        .arg(file_path.clone())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn() {
            Err(why) => panic!("Error spawning command `gsutil`: {:?}", why),
            Ok(process) => process.stdin
                .expect("Err: `process.stdin` in `gsutil cp - gs://electric-assets`"),
        };

    let size_transferred = field
        // accumulator: (file_path, 0i64) is `acc`
        .fold((file_path, 0i64), move |acc, bytes| {
            // Pipe/write bytestream to stdin (`gsutil`)
            let rt = stdin
                .write(bytes.as_ref())
                // write consumes the stream
                .map(|_| {
                    let (_file_path, _file_size) = acc;
                    // accumulate bytes transferred and return acc tuple
                    (_file_path, _file_size + bytes.len() as i64)
                })
                .map_err(|e| {
                    error!("stdin.write failed: {:?}", e);
                    error::MultipartError::Payload(error::PayloadError::Io(e))
                });
            future::result(rt)
        })
        .map_err(|e| error::ErrorInternalServerError(e));

    Box::new(size_transferred)
}


pub fn handle_multipart_item(
    item: multipart::MultipartItem<dev::Payload>,
) -> Box<Stream<Item = (String, i64), Error = Error>> {

    match item {
        multipart::MultipartItem::Field(field) => {

            let content_disposition = &field.content_disposition();
            debug!("content-disposition: {:?}", content_disposition);

            let file_path = match content_disposition {
                Some(f) => match f.get_filename() {
                    Some(filename) => format!("gs://electric-assets/{}", filename),
                    None => {
                        error!("No filename in {:?}", &field);
                        "temp.txt".to_string()
                    },
                },
                None => {
                    error!("No filename in {:?}", &field);
                    "temp.txt".to_string()
                },
            };

            Box::new(save_file(file_path, field).into_stream())

        },
        multipart::MultipartItem::Nested(mp) => {
            Box::new(
                mp.map_err(error::ErrorInternalServerError)
                    .map(handle_multipart_item)
                    .flatten()
            )
        }
    }
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadResponse {
    filepath: String,
    filelink: String,
    filesize: i64,
}
impl UploadResponse {
    fn new(filepath: String, filesize: i64) -> Self {
        let filelink = filepath.replace("gs://", "http://storage.googleapis.com/");
        UploadResponse {
            filepath: filepath,
            filelink: filelink,
            filesize: filesize,
        }
    }
}


pub fn upload(req: HttpRequest<AppState>) -> Box<Future<Item=HttpResponse, Error=Error>> {

    let res = req.multipart()
        .map_err(error::ErrorInternalServerError)
        .map(handle_multipart_item)
        .flatten()
        .collect()
        .map(|upload_resp: Vec<(String, i64)>| {

            let resp = upload_resp
                .iter()
                .map(|(fpath, fsize)| UploadResponse::new(fpath.to_owned(), fsize.to_owned()))
                .collect::<Vec<UploadResponse>>();

            HttpResponse::Ok().json(json!({
                "body": resp,
            }))
        })
        .map_err(|e| {
            error!("failed: {}", e);
            e
        });

    Box::new(res)
}


fn stream_to_gcloud(bytestream: &[u8], destination: &str) {
    let process = match std::process::Command::new("gsutil")
        .arg("cp")
        .arg("-")
        .arg(destination)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn() {
            Err(why) => panic!("Error spawning command `gsutil`: {:?}", why),
            Ok(process) => process,
        };

    match process.stdin.unwrap().write_all(bytestream) {
        Err(why) => panic!("Error: piping stream to `| gsutil cp - {}`:\t{:?}", &destination, why),
        Ok(s) => println!("Success: Piped stream to `| gsutil cp - {}`:\t{:?}", &destination, s),
    }

    let mut s = String::new();
    match process.stdout.unwrap().read_to_string(&mut s) {
        Err(why) => panic!("Could read `| gsutil cp - {}` stdout: {}", &destination, why),
        Ok(_) => println!("`| gsutil cp - {}` responded with: {:?}", &destination, s),
    }
}




