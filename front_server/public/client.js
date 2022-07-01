const MONITOR = document.getElementById('monitor');
const indicator = document.getElementById("socket");
const counter = document.getElementById("packet_count");
const read_button = document.getElementById("read_button");
const unread_button = document.getElementById("unread_button");
const get_interval_button = document.getElementById("get_interval");
/* const set_interval_button = document.getElementById("set_interval"); */
const interval_select = document.getElementById("interval_select");
const interval_options = document.querySelectorAll("#interval_select option");
const run_button = document.getElementById("run");
const stop_button = document.getElementById("stop");
const history_select = document.getElementById("history_select");
const history_options = document.getElementById("history_select");

let update_history = (socket) => {
  socket.emit("get_tables", "", (tables) => {
    console.log(tables)
    for (const table of tables) {
      console.log(table);
      let option = document.createElement("option");
      option.value = table["name"];
      option.text = table["name"];
      history_select.add(option, null)
    }
  });  
}

let xs = [];
let ys = [];
let rs = [];

Plotly.newPlot(
MONITOR,
[{ x: xs, y: ys }],
{ margin: { t: 0 } }
);

// Server connection event
const socket = io();
console.log("connected to the server");
update_history(socket);
socket.on("update_qcm", (data_str) => {
    console.log(data_str);
    let data = JSON.parse(data_str);
    data.forEach(function(datum){
        xs.push(datum["time"]);
        ys.push(datum["freq"]);
        rs.push(datum["rate"]);
    });
    Plotly.newPlot(
        MONITOR,
        [{ x: xs, y: ys }],
        { margin: { t: 0 } }
    );
});
socket.on("disconnect", () => {
    console.log("disconnected from server")
});

let count;
socket.on('connect', function() {
  count = 0;
  indicator.innerHTML= "connected";
  
  read_button.addEventListener("click", () => {
    socket.emit("read");
  });

  unread_button.addEventListener("click", () => {
    socket.emit("unread");
    xs = [];
    ys = [];
    rs = [];
  });
  
  get_interval_button.addEventListener("click", () => {
    socket.emit("get_interval", "", (response) => {
      console.log(response);
    });
  });

  interval_select.addEventListener("change", () => {
    let index = interval_select.selectedIndex;
    let interval = interval_options[index].value;
    socket.emit("set_interval", interval, (response) => {
      console.log(response);
    });
  });

  history_select.addEventListener("change", () => {
    let index = history_select.selectedIndex;
    let table = history_options[index].value;
    socket.emit("read", table, (data) => {
      console.log(data);
      xs, ys, rs = [], [], [];
      data.forEach(function(datum){
        xs.push(datum["time"]);
        ys.push(datum["freq"]);
        rs.push(datum["rate"]);
      });
      Plotly.newPlot(
        MONITOR,
        [{ x: xs, y: ys }],
        { margin: { t: 0 } }
      );
    });
  });
  
  run_button.addEventListener("click", () => {
    socket.emit("run", "", (response) => {
      console.log(response);
      update_history(socket);
    });
  });
  
  stop_button.addEventListener("click", () => {
    socket.emit("stop", "", (response) => {
      console.log(response);
    });
  });

});

socket.on('packet_count', function() {
  count++;
  counter.innerHTML = "packet counter: " + count;
});
