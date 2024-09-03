// Load HTML elements
const MONITOR_VIEW_0 = document.getElementById('monitor0');
const MONITOR_VIEW_1 = document.getElementById('monitor1');
const MONITOR_VIEW_2 = document.getElementById('monitor2');
const MONITOR_VIEW_3 = document.getElementById('monitor3');
const HISTORY_VIEW_0 = document.getElementById('history0');
const HISTORY_VIEW_1 = document.getElementById('history1');
const HISTORY_VIEW_2 = document.getElementById('history2');
const HISTORY_VIEW_3 = document.getElementById('history3');
const indicator = document.getElementById("socket");
const interval_select = document.getElementById("interval_select");
const interval_options = document.querySelectorAll("#interval_select option");
const run_button = document.getElementById("run");
const stop_button = document.getElementById("stop");
const history_select = document.getElementById("history_select");
const history_options = document.getElementById("history_select");
const save_button_history = document.getElementById("save_csv_history");
const remove_button = document.getElementById("remove");
const response_field = document.getElementById("response_field");

// Edit response field
function show_response(json_obj) {
  console.log(json_obj)
  let json_str = JSON.stringify(json_obj);
  response_field.value = json_str;
}

// General function to set options
function setOption(selectElement, value) {
  return [...selectElement.options].some((option, index) => {
      if (option.value == value) {
          selectElement.selectedIndex = index;
          return true;
      }
  });
}

// General function to use "zip" in JavaScript
function* zip(...args) {
  const length = args[0].length;
  for (let arr of args) {
      if (arr.length !== length){
          throw "Lengths of arrays are not eqaul.";
      }
  } 
  for (let index = 0; index < length; index++) {
      let elms = [];
      for (arr of args) {
          elms.push(arr[index]);
      }
      yield elms;
  }
}

// General function to remove all options in a select
function removeOptions(selectElement) {
  var i, L = selectElement.options.length - 1;
  for(i = L; i >= 0; i--) {
     selectElement.remove(i);
  }
}

// Function to download file
function download_csv(data, table_name) {
  for (let i = 0; i < 4; i++) {
    let content = "time(s),freq(Hz)\n";
    for (let [x, y] of zip(data[i][0], data[i][1])) {
      content += `${x},${y}\n`;
    }
    const blob = new Blob([ content ], { "type" : "text/csv" });
    const link = document.createElement("a");
    link.download = `${table_name}-ch${i + 1}.csv`;
    link.href = URL.createObjectURL(blob);
    link.click();
    URL.revokeObjectURL(link.href);
  }
}

// Plotly parameters
let layout = {
  title: 'QCM monitor',
  xaxis: { title: 'time / sec', automargin: true },
  yaxis: { title: 'frequency / Hz', automargin: true, tickformat: '.2f' },
  margin: { t: 96 }
};
let layout_hist = {
  title: 'QCM data viewer',
  xaxis: { title: 'time / sec', automargin: true },
  yaxis: { title: 'frequency / Hz', automargin: true, tickformat: '.2f' },
  margin: { t: 96 }
};
const config = { responsive: true };
const config_hist = { responsive: true };
let xs_live_0 = [];
let xs_live_1 = [];
let xs_live_2 = [];
let xs_live_3 = [];
let ys_live_0 = [];
let ys_live_1 = [];
let ys_live_2 = [];
let ys_live_3 = [];
let xs_0 = [];
let xs_1 = [];
let xs_2 = [];
let xs_3 = [];
let ys_0 = [];
let ys_1 = [];
let ys_2 = [];
let ys_3 = [];
Plotly.newPlot( MONITOR_VIEW_0, [{ x: xs_live_0, y: ys_live_0 }], layout, config );
Plotly.newPlot( MONITOR_VIEW_1, [{ x: xs_live_1, y: ys_live_1 }], layout, config );
Plotly.newPlot( MONITOR_VIEW_2, [{ x: xs_live_2, y: ys_live_2 }], layout, config );
Plotly.newPlot( MONITOR_VIEW_3, [{ x: xs_live_3, y: ys_live_3 }], layout, config );
Plotly.newPlot( HISTORY_VIEW_0, [{ x: xs_0, y: ys_0 }], layout_hist, config_hist );
Plotly.newPlot( HISTORY_VIEW_1, [{ x: xs_1, y: ys_1 }], layout_hist, config_hist );
Plotly.newPlot( HISTORY_VIEW_2, [{ x: xs_2, y: ys_2 }], layout_hist, config_hist );
Plotly.newPlot( HISTORY_VIEW_3, [{ x: xs_3, y: ys_3 }], layout_hist, config_hist );

// Define socket
const socket = io({reconnection: false});

// Socket disconnection event
socket.on("disconnect", () => {
  show_response("disconnected from server");
  indicator.value= "disconnected";
});

// Update table list
socket.on("update_table_list", (tables) => {
  console.log(tables);
  removeOptions(history_select);
  for (const table of tables) {
    const option = document.createElement("option");
    option.value = table["name"];
    option.text = table["name"];
    history_select.add(option, null)
  }
});

// Initialize monitor view
socket.on("init_monitor", () => {
  xs_live_0 = [];
  xs_live_1 = [];
  xs_live_2 = [];
  xs_live_3 = [];
  ys_live_0 = [];
  ys_live_1 = [];
  ys_live_2 = [];
  ys_live_3 = [];
});

// Update monitor view
socket.on("update_monitor", (data) => {
  data.forEach(function(datum) {
    let ch = datum["channel"];
    if (ch == 0) {
      xs_live_0.push(datum["start_time"]);
      ys_live_0.push(datum["freq"]);
    } else if (ch == 1) {
      xs_live_1.push(datum["start_time"]);
      ys_live_1.push(datum["freq"]);
    } else if (ch == 2) {
      xs_live_2.push(datum["start_time"]);
      ys_live_2.push(datum["freq"]);
    } else if (ch == 3) {
      xs_live_3.push(datum["start_time"]);
      ys_live_3.push(datum["freq"]);
    }
  });
  Plotly.newPlot(MONITOR_VIEW_0, [{ x: xs_live_0, y: ys_live_0 }], layout, config );
  Plotly.newPlot(MONITOR_VIEW_1, [{ x: xs_live_1, y: ys_live_1 }], layout, config );
  Plotly.newPlot(MONITOR_VIEW_2, [{ x: xs_live_2, y: ys_live_2 }], layout, config );
  Plotly.newPlot(MONITOR_VIEW_3, [{ x: xs_live_3, y: ys_live_3 }], layout, config );
});

// Update ineterval select
socket.on("update_interval", (interval) => {
  setOption(interval_select, interval)
});

// Socket connection event
socket.on('connect', function() {

  // Initialize graph and data
  xs_live_0 = [];
  xs_live_1 = [];
  xs_live_2 = [];
  xs_live_3 = [];
  ys_live_0 = [];
  ys_live_1 = [];
  ys_live_2 = [];
  ys_live_3 = [];
  Plotly.newPlot( MONITOR_VIEW_0, [{ x: xs_live_0, y: ys_live_0 }], layout, config );
  Plotly.newPlot( MONITOR_VIEW_1, [{ x: xs_live_1, y: ys_live_1 }], layout, config );
  Plotly.newPlot( MONITOR_VIEW_2, [{ x: xs_live_2, y: ys_live_2 }], layout, config );
  Plotly.newPlot( MONITOR_VIEW_3, [{ x: xs_live_3, y: ys_live_3 }], layout, config );

  // Show connected
  show_response("connected to server");
  indicator.value= "connected";

  // Interval select
  interval_select.addEventListener("change", () => {
    let index = interval_select.selectedIndex;
    let interval = interval_options[index].value;
    socket.emit("set_interval", interval, (response) => {
      show_response(response);
    });
  });

  // Database table select
  history_select.addEventListener("change", () => {
    let index = history_select.selectedIndex;
    let table = history_options[index].value;
    socket.emit("read_db", table, (data) => {
      show_response(`Got data from ${table}`);
      xs_0 = data[0][0];
      ys_0 = data[0][1];
      xs_1 = data[1][0];
      ys_1 = data[1][1];
      xs_2 = data[2][0];
      ys_2 = data[2][1];
      xs_3 = data[3][0];
      ys_3 = data[3][1];
      layout.title = table;
      Plotly.newPlot(HISTORY_VIEW_0, [{ x: xs_0, y: ys_0 }], layout_hist, config_hist);
      Plotly.newPlot(HISTORY_VIEW_1, [{ x: xs_1, y: ys_1 }], layout_hist, config_hist);
      Plotly.newPlot(HISTORY_VIEW_2, [{ x: xs_2, y: ys_2 }], layout_hist, config_hist);
      Plotly.newPlot(HISTORY_VIEW_3, [{ x: xs_3, y: ys_3 }], layout_hist, config_hist);
    });
  });

  // Run button
  run_button.addEventListener("click", () => {
    let index = interval_select.selectedIndex;
    let interval = interval_options[index].value;
    socket.emit("set_interval", interval, (response) => {
      show_response(response);
      socket.emit("run_multi", interval, (response) => {
        show_response(response);
        if ("Success" in response) {
          xs_live_0 = [];
          xs_live_1 = [];
          xs_live_2 = [];
          xs_live_3 = [];
          ys_live_0 = [];
          ys_live_1 = [];
          ys_live_2 = [];
          ys_live_3 = [];
        }
      });
    });
  });

  // Stop button
  stop_button.addEventListener("click", () => {
    socket.emit("stop", "", (response) => {
      show_response(response);
    });
  });

  // Save button for table list
  save_button_history.addEventListener("click", () => {
    let index = history_select.selectedIndex;
    let table_name = history_options[index].value;
    table_name = table_name.replaceAll(":", "-");
    download_csv([[xs_0, ys_0], [xs_1, ys_1], [xs_2, ys_2], [xs_3, ys_3]], table_name);
  });

  // Remove button
  remove_button.addEventListener("click", () => {
    let index = history_select.selectedIndex;
    let table_name = history_options[index].value;
    if(window.confirm(`Are you sure to remove ${table_name}？`)){
      socket.emit("remove", table_name, (response) => {
        show_response(response);
      });
    }
  }); 

});
