const MONITOR_VIEW = document.getElementById('monitor');
const HISTORY_VIEW = document.getElementById('history');
const indicator = document.getElementById("socket");
/* const start_time = document.getElementById("start_time"); */
/* const counter = document.getElementById("packet_count"); */
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
const save_button_run = document.getElementById("save_csv_run");
const save_button_history = document.getElementById("save_csv_history");

let layout = {
  title: 'QCM monitor',
  xaxis: { title: 'time / sec', automargin: true },
  yaxis: { title: 'frequency / Hz', automargin: true },
  margin: { t: 96 }
};
let config = { responsive: true };

let update_history = (socket) => {
  for (var i=0; i<history_select.length; i++) {
    history_select.remove(i);
  }
  socket.emit("get_tables", "", (tables) => {
    for (const table of tables) {
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

let xs_live = ys_live = rs_live = [];
let xs = ys = rs = [];
Plotly.newPlot( MONITOR_VIEW, [{ x: xs_live, y: ys_live }], layout, config );
Plotly.newPlot( HISTORY_VIEW, [{ x: xs, y: ys }], layout, config );

// Server connection event
const socket = io();
console.log("connected to the server");
update_history(socket);
socket.on("update_qcm", (data_str) => {
    let data = JSON.parse(data_str);
    data.forEach(function(datum){
        xs_live.push(datum["time"]);
        ys_live.push(datum["freq"]);
        rs_live.push(datum["rate"]);
    });
    layout.title = "Transfer rate: " + rs_live[rs_live.length - 1];
    Plotly.newPlot(MONITOR_VIEW, [{ x: xs_live, y: ys_live }], layout, config );
});
socket.on("disconnect", () => {
    console.log("disconnected from server")
});

let count;
socket.on('connect', function() {
  
  // Get Interval or current running DB
  socket.emit("get_interval", "", (response) => {
    console.log(response);
    if ("Success" in response) {
      let interval = response["Success"]["GotValue"];
      setOption(interval_select, interval);
    } else if ("Failure" in response) {
      let table_name = response["Failure"]["Busy"];
      
    }
  });
  
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
      layout.title = table;
      Plotly.newPlot(HISTORY_VIEW, [{ x: xs, y: ys }], layout, config);
    });
  });
  
  run_button.addEventListener("click", () => {
    socket.emit("run", "", (response) => {
      console.log(response);
      let table_name = response["Success"]["SaveTable"];
      xs_live = [];
      ys_live = [];
      rs_live = [];
      socket.emit("loop", table_name);
    });
  });
  
  stop_button.addEventListener("click", () => {
    socket.emit("stop", "", (response) => {
      console.log(response);
    });
  });

  save_button_run.addEventListener("click", () => {
    socket.emit("save", "", (response) => {
      console.log(response);
    })
  });

  save_button_history.addEventListener("click", () => {
    let index = history_select.selectedIndex;
    let table = history_options[index].value;
    socket.emit("save", table, (response) => {
      console.log(response);
    })
  });

});

/*
socket.on('packet_count', function() {
  count++;
  counter.innerHTML = "packet counter: " + count;
});
*/