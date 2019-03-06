
use std::collections::HashSet;
use futures::{Future, Poll, Stream};

use bytes::BytesMut;
use bytes::IntoBuf;
use prost::DecodeError as ProtoBufDecodeError;
use prost::EncodeError as ProtoBufEncodeError;
use prost::Message;

use actix_web::dev::HttpResponseBuilder;
use actix_web::error::{Error, PayloadError, ResponseError};
use actix_web::http::header::CONTENT_TYPE;
use actix_web::{
    HttpMessage, HttpRequest, HttpResponse, Responder,
    AsyncResponder,
    Error as AWError
};
use crate::AppState;
use crate::MyObj;



#[derive(Debug)]
pub struct ProtoBuf<T: Message>(pub T);

impl<T: Message> Responder for ProtoBuf<T> {
    type Item = HttpResponse;
    type Error = Error;

    fn respond_to<S>(self, _: &HttpRequest<S>) -> Result<HttpResponse, Error> {
        let mut buf = Vec::new();
        self.0
            .encode(&mut buf)
            .map_err(|e| Error::from(ProtoBufPayloadError::Serialize(e)))
            .and_then(|()| {
                Ok(HttpResponse::Ok()
                    .content_type("application/protobuf")
                    .body(buf)
                    .into())
            })
    }
}

pub struct ProtoBufBody<U: Message + Default> {
    fut: Box<Future<Item = U, Error = ProtoBufPayloadError>>,
}

impl<U: Message + Default + 'static> ProtoBufBody<U> {
    /// Create `ProtoBufBody` for request.
    pub fn new(req: &HttpRequest<AppState>) -> Self {
        // Turns body into byteslice, buffer it, then wrap it in a future
        let fut = req
            .payload() // Message stream: payload() -> Stream<Item=Bytes, Error=PayloadError>
            .map_err(|e| ProtoBufPayloadError::Payload(e)) // raise error on PayloadError
            .fold(BytesMut::new(), move |mut body, chunk| {
                // Fold message stream into a BytesMut (memory slice)
                body.extend_from_slice(&chunk);
                // clone + append chunk, then return
                Ok::<_, ProtoBufPayloadError>(body)
            })  // Insert into buffer, then decode Message as value
            .and_then(|body| {
                Ok(<U>::decode(&mut body.into_buf())?)
            });

        ProtoBufBody { fut: Box::new(fut) }
    }
}

impl<U: Message + Default + 'static> Future for ProtoBufBody<U> where {
    type Item = U;
    type Error = ProtoBufPayloadError;

    fn poll(&mut self) -> Poll<U, ProtoBufPayloadError> {
        self.fut.poll()
    }
}

pub trait ProtoBufResponseBuilder {
    fn protobuf<T: Message>(&mut self, value: T) -> Result<HttpResponse, Error>;
}

impl ProtoBufResponseBuilder for HttpResponseBuilder {
    fn protobuf<T: Message>(&mut self, value: T) -> Result<HttpResponse, Error> {
        self.header(CONTENT_TYPE, "application/protobuf");

        let mut body = Vec::new();
        // encode value, and insert into body vector
        value.encode(&mut body)
            .map_err(|e| ProtoBufPayloadError::Serialize(e))?;

        println!("\n4. body: {:?}\n", body);
        Ok(self.body(body))
    }
}


#[derive(Fail, Debug)]
pub enum ProtoBufPayloadError {
    /// Payload size is bigger than 256k
    #[fail(display = "Payload size is bigger than 256k")]
    Overflow,
    /// Content type error
    #[fail(display = "Content type error: {}", _0)]
    ContentType(String),
    /// Serialize error
    #[fail(display = "ProtoBuf serialize error: {}", _0)]
    Serialize(#[cause] ProtoBufEncodeError),
    /// Deserialize error
    #[fail(display = "ProtoBuf deserialize error: {}", _0)]
    Deserialize(#[cause] ProtoBufDecodeError),
    /// Payload error
    #[fail(display = "Error that occur during reading payload: {}", _0)]
    Payload(#[cause] PayloadError),
}

impl ResponseError for ProtoBufPayloadError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            ProtoBufPayloadError::Overflow => HttpResponse::PayloadTooLarge().into(),
            _ => HttpResponse::BadRequest().into(),
        }
    }
}

impl From<PayloadError> for ProtoBufPayloadError {
    fn from(err: PayloadError) -> ProtoBufPayloadError {
        ProtoBufPayloadError::Payload(err)
    }
}

impl From<ProtoBufDecodeError> for ProtoBufPayloadError {
    fn from(err: ProtoBufDecodeError) -> ProtoBufPayloadError {
        ProtoBufPayloadError::Deserialize(err)
    }
}
