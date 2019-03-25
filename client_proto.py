# just start server and run client.py

# wget https://github.com/google/protobuf/releases/download/v3.5.1/protobuf-python-3.5.1.zip
# unzip protobuf-python-3.5.1.zip.1
# cd protobuf-3.5.1/python/
# python3.6 setup.py install

# pip3.6 install --upgrade pip
# pip3.6 install aiohttp

#!/usr/bin/env python
from proto_schemas import test_pb2
import traceback
import sys

import asyncio
import aiohttp
import requests


# async def go(loop):
#     print("Start event loop: go(loop): ")
#     async with aiohttp.ClientSession(loop=loop) as session:
#         await post_protobuf(session)
#
# async def post_protobuf(session):
#
#     obj = test_pb2.MyObj()
#     obj.number = 433678990
#     obj.name = 'Alicia'
#
#     async with session.post('http://localhost:7070/ws/pb/pb/stuff',
#         data=obj.SerializeToString(),
#         headers={"content-type": "application/protobuf"}) as resp:
#
#         print("Sent protobuf data to server: ")
#         print(obj)
#         print(resp.status)
#
#         data = await resp.read()
#         receiveObj = test_pb2.MyObj()
#         receiveObj.ParseFromString(data)
#         print("Received protobuf response from server + deserialized successfully:")
#         print(receiveObj)
#         print("Here's the name: ", receiveObj.name)
#         print("Here's the number: ", receiveObj.number)
#
#
# loop = asyncio.get_event_loop()
# loop.run_until_complete(go(loop))
# loop.close()


# In: Json
# Out: Json to websocket clients
requests.post(
    "http://127.0.0.1:7070/ws/json/json/stuff",
    json={"name": "Jennifer", "number": 288000111}
).json()

# In: Json
# Out: Protobuf to websocket clients
requests.post(
    "http://127.0.0.1:7070/ws/json/pb/stuff",
    json={"name": "Belle", "number": 422222222}
).json()


print("Sent: JSON, broadcast Protobuf to websocket clients")
send_obj = test_pb2.MyObj()
send_obj.number = 433678990
send_obj.name = 'Alicia'

# In: Protobuf
# Out: Protobuf to websocket clients
resp = requests.post(
    "http://127.0.0.1:7070/ws/pb/pb/stuff",
    headers={ 'content-type': 'application/protobuf' },
    data=send_obj.SerializeToString(),
)

print("\nSent protobuf data to server: ")
print(send_obj)
print(resp)

data = resp.content
receiveObj = test_pb2.MyObj()
receiveObj.ParseFromString(data)
print("Received protobuf response from server + deserialized successfully:")
print("Here's the name: ", receiveObj.name)
print("Here's the number: ", receiveObj.number)

