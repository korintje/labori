// Load HTML elements
const MONITOR_VIEW = document.getElementById('monitor');
const HISTORY_VIEW = document.getElementById('history');
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
function download_csv(xs, ys, table_name) {
  let content = "time(sec),frequency(Hz)\n";
  for (let [x, y] of zip(xs, ys)) {
    content += `${x},${y}\n`;
  }
  const blob = new Blob([ content ], { "type" : "text/csv" });
  const link = document.createElement("a");
  link.download = `${table_name}.csv`;
  link.href = URL.createObjectURL(blob);
  link.click();
  URL.revokeObjectURL(link.href);
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
let xs_live = [];
let ys_live = [];
let rs_live = [];
let xs = [];
let ys = [];
let rs = [];
Plotly.newPlot( MONITOR_VIEW, [{ x: xs_live, y: ys_live }], layout, config );
Plotly.newPlot( HISTORY_VIEW, [{ x: xs, y: ys }], layout_hist, config_hist );

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
  xs_live = [];
  ys_live = [];
  rs_live = [];
});

// Update monitor view
socket.on("update_monitor", (data) => {
  data.forEach(function(datum){
    xs_live.push(datum["time"]);
    ys_live.push(datum["freq"]);
    rs_live.push(datum["rate"]);
  });
  const sent_byte_amount = rs_live[rs_live.length - 1];
  layout.title = `Network occupancy: ${sent_byte_amount} / 1024`; 
  Plotly.newPlot(MONITOR_VIEW, [{ x: xs_live, y: ys_live }], layout, config );
});

// Update ineterval select
socket.on("update_interval", (interval) => {
  setOption(interval_select, interval)
});

// Socket connection event
socket.on('connect', function() {

  // Initialize graph and data
  xs_live = [];
  ys_live = [];
  rs_live = [];
  Plotly.newPlot( MONITOR_VIEW, [{ x: xs_live, y: ys_live }], layout, config );

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
      xs = data[0];
      ys = data[1];
      layout.title = table;
      Plotly.newPlot(HISTORY_VIEW, [{ x: xs, y: ys }], layout_hist, config_hist);
    });
  });

  // Run button
  run_button.addEventListener("click", () => {
    let index = interval_select.selectedIndex;
    let interval = interval_options[index].value;
    socket.emit("set_interval", interval, (response) => {
      show_response(response);
      socket.emit("run", interval, (response) => {
        show_response(response);
        if ("Success" in response) {
          xs_live = [];
          ys_live = [];
          rs_live = [];
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
    download_csv(xs, ys, table_name);
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
