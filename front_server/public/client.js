// Load HTML elements
const MONITOR_VIEW = document.getElementById('monitor');
const HISTORY_VIEW = document.getElementById('history');
const indicator = document.getElementById("socket");
const get_interval_button = document.getElementById("get_interval");
const interval_select = document.getElementById("interval_select");
const interval_options = document.querySelectorAll("#interval_select option");
const run_button = document.getElementById("run");
const stop_button = document.getElementById("stop");
const history_select = document.getElementById("history_select");
const history_options = document.getElementById("history_select");
const save_button_run = document.getElementById("save_csv_run");
const save_button_history = document.getElementById("save_csv_history");

// General function to set options
function setOption(selectElement, value) {
  return [...selectElement.options].some((option, index) => {
      if (option.value == value) {
          selectElement.selectedIndex = index;
          return true;
      }
  });
}

// Plotly parameters
let layout = {
  title: 'QCM monitor',
  xaxis: { title: 'time / sec', automargin: true },
  yaxis: { title: 'frequency / Hz', automargin: true },
  margin: { t: 96 }
};
const config = { responsive: true };
let xs_live = ys_live = rs_live = [];
let xs = ys = rs = [];
Plotly.newPlot( MONITOR_VIEW, [{ x: xs_live, y: ys_live }], layout, config );
Plotly.newPlot( HISTORY_VIEW, [{ x: xs, y: ys }], layout, config );

// Define socket
const socket = io();

// Socket disconnection event
socket.on("disconnect", () => {
  console.log("disconnected from server")
  indicator.innerHTML= "disconnected";
});

// Update table list
socket.on("update_table_list", (tables) => {
  for (var i=0; i<history_select.length; i++) {
    history_select.remove(i);
  }
  for (const table of tables) {
    const option = document.createElement("option");
    option.value = table["name"];
    option.text = table["name"];
    history_select.add(option, null)
  }
});

// Update monitor view
socket.on("update_monitor", (data) => {
    console.log(data);
    data.forEach(function(datum){
        xs_live.push(datum["time"]);
        ys_live.push(datum["freq"]);
        rs_live.push(datum["rate"]);
    });
    layout.title = "Transfer rate: " + rs_live[rs_live.length - 1];
    Plotly.newPlot(MONITOR_VIEW, [{ x: xs_live, y: ys_live }], layout, config );
});

// Socket connection event
socket.on('connect', function() {

  // Show connected
  console.log("connected to server");
  indicator.innerHTML= "connected";
  
  // Get interval button
  get_interval_button.addEventListener("click", () => {
    socket.emit("get_interval", "", (response) => {
      console.log(response);
    });
  });

  // Interval select
  interval_select.addEventListener("change", () => {
    let index = interval_select.selectedIndex;
    let interval = interval_options[index].value;
    socket.emit("set_interval", interval, (response) => {
      console.log(response);
    });
  });

  // Database table select
  history_select.addEventListener("change", () => {
    let index = history_select.selectedIndex;
    let table = history_options[index].value;
    socket.emit("read_db", table, (data) => {
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

  // Run button
  run_button.addEventListener("click", () => {
    socket.emit("run", "", (response) => {
      console.log(response);
      xs_live = [];
      ys_live = [];
      rs_live = [];
    });
  });

  // Stop button
  stop_button.addEventListener("click", () => {
    socket.emit("stop", "", (response) => {
      console.log(response);
    });
  });

  // Save button for monitor
  save_button_run.addEventListener("click", () => {
    socket.emit("save", "", (response) => {
      console.log(response);
    })
  });

  // Save button for table list
  save_button_history.addEventListener("click", () => {
    let index = history_select.selectedIndex;
    let table = history_options[index].value;
    socket.emit("save", table, (response) => {
      console.log(response);
    })
  });

});
