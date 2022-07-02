const app  = require("express")();
const express = require("express");
const http = require("http").Server(app);
const io = require("socket.io")(http);
const sqlite3 = require("sqlite3");
const net = require('net');

/* Non-blocking setting loading 
const fs = require("fs");
const toml = require("toml");
let CONFIG = {};
fs.readFile("./config.toml", "utf-8", (err, obj) => {
  const c = toml.parse(obj);
  CONFIG.TCP_PORT = c.TCP_client.port;
  CONFIG.WS_PORT = c.WS_server.port;
  CONFIG.SAMPLE_RATE = c.database.sampling_rate;
});
*/
TCP_PORT = 50001;
WS_PORT = 3000;
SAMPLE_RATE = 100;

// Set DB connection
const db = new sqlite3.Database("../back_client/Iwatsu.db");

// Pass response from back client to websocket client
const path_through = (callback, cmd) => {
  const client = net.connect(TCP_PORT, 'localhost', () => {
    console.log('connected to TCP server');
    client.write(cmd);
  });
  client.on('data', data => {
    console.log('client-> ' + data);
    callback(JSON.parse(data));
  });  
};

// SQL streaming function
let stream = (socket) => {
  db.all(`select *, rowid from '${table}' where rowid>${last_rowid}`, (_err, data) => {
    let last_row = data[data.length - 1];
    if (last_row !== undefined) {
      last_rowid = last_row["rowid"];
      data_str = JSON.stringify(data);
      socket.emit("update_qcm", data_str);
      /* socket.emit('packet_count'); */
    }
  });
}

// Work as TCP client
const tcp_client = net.connect(TCP_PORT, 'localhost', () => {
  console.log('connected to TCP server');
  tcp_client.write(String.raw`{"Get": {"key": "Interval"}}`);
});

tcp_client.on('data', data => {
  console.log('TCP client -> ' + data);
  if ("Success" in data) {
    let interval = init_res["Success"]["GotValue"];
    setOption(interval_select, interval);
  } else if ("Failure" in data) {
    let table_name = data["Failure"]["Busy"];
  }
});

tcp_client.on('close', () => {
  console.log('client-> connection is closed');
});

// Return index.html and static files directory path
app.get('/app/qcm', function(req, res){
  res.sendFile(__dirname + '/index.html');
});
app.use('/app/qcm', express.static(__dirname + "/public"));

// Client connection event
io.on("connection", (socket, resback) => {

  // Initialization
  let streaming;
  console.log("a client connected")
  let last_rowid = 0;
  /*path_through(resback, `{"Get": {"key": "Interval"}}`);*/
  const client = net.connect(TCP_PORT, 'localhost', () => {
    console.log('connected to TCP server');
    client.write(String.raw`{"Get": {"key": "Interval"}}`);
  });

  // Reset last Row ID if client disconnected
  socket.on("disconnect", () => {
    last_rowid = 0;
    console.log("client disconnected")
  });

  // Read history
  socket.on("read", (table, callback)  => {
    console.log(`select * from '${table}'`);
    db.all(`select * from '${table}'`, (_err, data) => {
      console.log(`${data.length} data has sent.`);
      callback(data);
    });
  });  

  // Start SQL polling loop
  socket.on("loop", (table, callback)  => {
    console.log("Read SQL signal from : " + socket.id);
    streaming = setInterval(
      () => { stream(socket); }, SAMPLE_RATE
    );
    // callback("Read");
  });
  
  // Get Interval
  socket.on('get_interval', (_arg, callback) => {
    path_through(callback, `{"Get": {"key": "Interval"}}`);
  });
  
  // Set Interval
  socket.on('set_interval', (interval, callback) => {
    path_through(callback, `{"Set": {"key": "Interval", "value": "${interval}"}}`);
  });
  
  // Run measurement
  socket.on('run', (_arg, callback) => {
    path_through(callback, `{"Run": {}}`);
  }); 
  
  // Stop measurement
  socket.on('stop', (_arg, callback) => {
    console.log('Stop signal from: ' + socket.id);
    clearInterval(streaming);
    path_through(callback, `{"Stop": {}}`);
  }); 
  
  // Get table list from DB
  socket.on('get_tables', (_arg, callback) => {
    db.serialize(function () {
      db.all("select name from sqlite_master where type='table'", function (err, tables) {
        callback(tables);
      });
    });
  });

});

// Listen at 127.0.0.1:3000
http.listen(WS_PORT, function(){
  console.log(`listening on *:${WS_PORT}`);
});




