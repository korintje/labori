// Import mudules
const app  = require("express")();
const express = require("express");
const http = require("http").Server(app);
const io = require("socket.io")(http);
const sqlite3 = require("sqlite3");
const net = require('net');

// Global constants
TCP_PORT = 50001;
WS_PORT = 3000;
SAMPLE_RATE = 100;

// Database connection
const db = new sqlite3.Database("../back_client/Iwatsu.db");

// Return index.html and static files directory path
app.get('/app/qcm', (_, res) => { res.sendFile(__dirname + '/index.html'); });
app.use('/app/qcm', express.static(__dirname + "/public"));
app.get('/app/qcm-multi', (_, res) => { res.sendFile(__dirname + '/index-multi.html'); });
app.use('/app/qcm-multi', express.static(__dirname + "/public"));

// Listen websocket port
http.listen(WS_PORT, function(){
  console.log(`listening on *:${WS_PORT}`);
});

// Be a client of TCP server
const connect_TCP = (cmd) => {
  const client = net.connect(TCP_PORT, 'localhost', () => {
    console.log('connected to TCP server');
    client.write(cmd);
  });
  return client;
};

// Pass response from back client to websocket client
const path_through = (callback, cmd) => {
  const client = connect_TCP(cmd);
  client.on('data', data => {
    console.log('Received from TCP server: ' + data);
    callback(JSON.parse(data));
  });  
};

// Check wheather polling process is running
const check_run = (socket, state) => {
  const client = connect_TCP(`{"Get": {"key": "Interval"}}`);
  client.on('data', data => {
    console.log('Received from TCP server: ' + data);
    const json_data = JSON.parse(data);
    if ("Success" in json_data) {
      const interval = json_data["Success"]["GotValue"];
      socket.emit("update_interval", interval);
    } else if ("Failure" in json_data) {
      if ("Busy" in json_data["Failure"]) {
        const table_name = json_data["Failure"]["Busy"]["table_name"];
        const interval = json_data["Failure"]["Busy"]["interval"];
        socket.emit("update_interval", interval); 
        state["last_rowid"] = 0;
        state["streaming"] = setInterval(
          () => { stream(socket, table_name, state); }, SAMPLE_RATE
        );
      }
    }
  });  
};

//  Get and update interval value
const get_and_update_interval = (socket) => {
  const client = connect_TCP(`{"Get": {"key": "Interval"}}`);
  client.on('data', data => {
    console.log('Received from TCP server: ' + data);
    const json_data = JSON.parse(data);
    if ("Success" in json_data) {
      const interval = json_data["Success"]["GotValue"];
      socket.emit("update_interval", interval);
    }
  }); 
}

// Stream data from given database
const stream = (socket, table_name, state) => {
  db.all(`select *, rowid from '${table_name}' where rowid>${state["last_rowid"]}`, (_e, data) => {
    if (data !== undefined) {
      let last_row = data[data.length - 1];
      if (last_row !== undefined) {
        state["last_rowid"] = last_row["rowid"];
        socket.emit("update_monitor", data);
      }
    }
  });
};

// ----- Old function for Backup --------
// Get table list from DB
// const get_tables = (socket) => {
//   db.all("select name from sqlite_master where type='table'", function (_e, tables) {
//     socket.emit("update_table_list", tables);
//   });
// };
// ----- ----------------------- --------

// Get table names from registry table
const get_tables = (socket) => {
  db.all("SELECT table_name FROM registry", (err, registryRows) => {
    if (err) {
      console.error("Error fetching from registry table:", err);
      socket.emit("update_table_list", []);
      return;
    }
    const tableNames = registryRows.map(row => row.table_name);
    socket.emit("update_table_list", tableNames);
  });
};


// Client connection event
io.on("connection", (socket) => {

  // Streaming state
  let state = {"streaming": () => {}, "last_rowid": 0,}

  // Check run and interval
  console.log("a client connected")
  check_run(socket, state);
  get_tables(socket);

  // Reset last Row ID if client disconnected
  socket.on("disconnect", () => {
    state["last_rowid"] = 0;
    console.log("client disconnected")
  });

  // Read database
  socket.on("read_db", (table, callback)  => {
    console.log(`select time,freq from '${table}'`);
    db.all(`select time,freq from '${table}'`, (_err, data) => {
      if (data !== undefined) {
        let ts = [];
        let fs = [];
        data.forEach(datum => {
          ts.push(datum["time"]);
          fs.push(datum["freq"]);          
        });
        console.log(`${data.length} data has sent.`);
        callback([ts,fs]);
      }
    });
  });

  // Read database multichannel
  socket.on("read_db_multi", (table, callback)  => {
    console.log(`select time,channel,freq from '${table}'`);
    db.all(`select time,channel,freq from '${table}'`, (_err, data) => {
      if (data !== undefined) {
        let ts_0 = [];
        let fs_0 = [];
        let ts_1 = [];
        let fs_1 = [];
        let ts_2 = [];
        let fs_2 = [];
        let ts_3 = [];
        let fs_3 = [];
        data.forEach(datum => {
          let ch = datum["channel"];
          if (ch == 0) {
            ts_0.push(datum["time"]);
            fs_0.push(datum["freq"]);
          } else if (ch == 1) {
            ts_1.push(datum["time"]);
            fs_1.push(datum["freq"]);
          } else if (ch == 2) {
            ts_2.push(datum["time"]);
            fs_2.push(datum["freq"]);
          } else if (ch == 3) {
            ts_3.push(datum["time"]);
            fs_3.push(datum["freq"]);
          }      
        });
        console.log(`${data.length} data has sent.`);
        callback([[ts_0,fs_0],[ts_1,fs_1],[ts_2,fs_2],[ts_3,fs_3]]);
      }
    });
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
  socket.on('run', (duration, callback) => {
    
    /* To use internal clock in the counter: */
    // const client = connect_TCP(`{"Run": {}}`);
    
    /* To use external clock e.g. Raspberry pi: */
    const client = connect_TCP(`{"RunExt": {"duration": "${duration}"}}`)

    client.on('data', data => {
      console.log('Received from TCP server: ' + data);
      const json_data = JSON.parse(data);
      if ("Success" in json_data) {
        const table_name = json_data["Success"]["SaveTable"];
        state["last_rowid"] = 0;
        state["streaming"] = setInterval(
          () => { stream(socket, table_name, state); }, SAMPLE_RATE
        );
      } else if ("Failure" in json_data) {
        ;
      }
      callback(json_data);
    });
    get_tables(socket);
  });

  // Run measurement
  socket.on('run_multi', (duration, callback) => {

    /* To use external clock e.g. Raspberry pi: */
    const client = connect_TCP(`{"RunMulti": {"duration": "${duration}"}}`)

    client.on('data', data => {
      console.log('Received from TCP server: ' + data);
      const json_data = JSON.parse(data);
      if ("Success" in json_data) {
        const table_name = json_data["Success"]["SaveTable"];
        state["last_rowid"] = 0;
        state["streaming"] = setInterval(
          () => { stream(socket, table_name, state); }, SAMPLE_RATE
        );
      } else if ("Failure" in json_data) {
        console.log("Failure in json data");
      }
      callback(json_data);
    });
    get_tables(socket);
  });

  // Explicet update table list
  /*
  socket.on('get_table_list', (_arg, callback) => {
    get_tables(socket);
  });
  */

  // Stop measurement
  socket.on('stop', (_arg, callback) => {
    const client = connect_TCP(`{"Stop": {}}`);
    client.on('data', data => {
      console.log('Received from TCP server: ' + data);
      const json_data = JSON.parse(data);
      if ("Success" in json_data) {
        state["last_rowid"] = 0;
        clearInterval(state["streaming"]);
        get_and_update_interval(socket);
        get_tables(socket);
      } else if ("Failure" in json_data) {
        ;
      }
      callback(json_data);
    });
  });  

  // Remove table
  socket.on('remove', (table_name, callback) => {
    db.run(`drop table if exists '${table_name}'`);
    get_tables(socket);
    callback(JSON.parse(`{"TableRemoved": "${table_name}"}`));
    console.log(`Table "${table_name}" removed`);
  });

  /*
  // Update state
  socket.on('update', (_arg, callback) => {
    check_run(socket, state);
    get_tables(socket);
    callback(`{"Update": {}}`);
  });
  */ 

});
