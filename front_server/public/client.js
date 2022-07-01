const MONITOR_VIEW = document.getElementById('monitor');
const HISTORY_VIEW = document.getElementById('history');
const indicator = document.getElementById("socket");
const start_time = document.getElementById("start_time");
const counter = document.getElementById("packet_count");
/* const read_button = document.getElementById("read_button"); */
/* const unread_button = document.getElementById("unread_button"); */
const get_interval_button = document.getElementById("get_interval");
/* const set_interval_button = document.getElementById("set_interval"); */
const interval_select = document.getElementById("interval_select");
const interval_options = document.querySelectorAll("#interval_select option");
const run_button = document.getElementById("run");
const stop_button = document.getElementById("stop");
const history_select = document.getElementById("history_select");
const history_options = document.getElementById("history_select");

let update_history = (socket) => {
  for (var i=0; i<history_select.length; i++) {
    history_select.remove(i);
  }
  socket.emit("get_tables", "", (tables) => {
    for (const table of tables) {
      console.log(table);
      let option = document.createElement("option");
      option.value = table["name"];
      option.text = table["name"];
      history_select.add(option, null)
    }
  });  
}

function setOption(selectElement, value) {
  return [...selectElement.options].some((option, index) => {
      if (option.value == value) {
          selectElement.selectedIndex = index;
          return true;
      }
  });
}

let xs = [];
let ys = [];
let rs = [];

Plotly.newPlot(
  MONITOR_VIEW,
  [{ x: xs, y: ys }],
  { margin: { t: 0 } }
);
  

Plotly.newPlot(
  HISTORY_VIEW,
  [{ x: xs, y: ys }],
  { margin: { t: 0 } }
);

// Server connection event
const socket = io();
console.log("connected to the server");
update_history(socket);
socket.on("update_qcm", (data_str) => {
    xs = [];
    ys = [];
    rs = [];
    console.log(data_str);
    let data = JSON.parse(data_str);
    data.forEach(function(datum){
        xs.push(datum["time"]);
        ys.push(datum["freq"]);
        rs.push(datum["rate"]);
    });
    Plotly.update(
        MONITOR_VIEW,
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
      console.log(`${data.length} data has received.`);
      xs = [];
      ys = [];
      rs = [];
      data.forEach(function(datum){
        xs.push(datum["time"]);
        ys.push(datum["freq"]);
        rs.push(datum["rate"]);
      });
      Plotly.newPlot(
        HISTORY_VIEW,
        [{ x: xs, y: ys }],
        { margin: { t: 0 } }
      );
    });
  });
  
  run_button.addEventListener("click", () => {
    socket.emit("run", "", (response) => {
      console.log(response);
      let table_name = response["Success"]["SaveTable"];
      console.log(table_name);
      indicator.innerHTML= table_name;
      socket.emit("loop", table_name);
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
