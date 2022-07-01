//set background canvas
let canvas_back = document.getElementById("canvas_back");
let context_back = canvas_back.getContext("2d");

//draw hanoi moves
var socket = io();
socket.on("update_qcm", function(data){

  data.forEach(function(datum){
    let time = datum["time"]
    let freq = datum["freq"]
    let rate = datum["rate"]
  });

  data.state.forEach(function(disc_list, peg){
    disc_list.forEach(function(disc_num, order){
      disc_w = disc_num * (max_disc_w / data.total_discs) + min_disc_w;
      disc_h = (peg_l - min_margin) / data.total_discs;
      disc_x = peg_xs[peg] - disc_w / 2;
      disc_y = peg_y + peg_l - disc_h * (1 + order);
      disc_color = (disc_num + 18) * (-360) / data.total_discs
      context_main.fillStyle = "hsl(" + disc_color + ", 100%, 50%)";
      context_main.globalAlpha = 0.6;
      context_main.fillRect(disc_x, disc_y, disc_w, disc_h);
      context_main.fillStyle = 'rgb(255,255,255)';
      context_main.globalAlpha = 1;
      textWidth = context_main.measureText(disc_num).width;
      context_main.fillText(disc_num, peg_xs[peg] - textWidth / 2, peg_y + peg_l - disc_h * (order + 1/8));
    });
  });
});

//load history
let elem = document.getElementById("history");
let listObj = "<li>2018-10-14 17:16:51: 世界の終わりまでのカウントを開始しました。</li>";
$(function() {
  $.getJSON("hanoi/save_64.json" , function(data) {
    data.history_ja.forEach(function(message){
      listObj += "<li>" + message + "</li>";
    });
    elem.innerHTML = listObj;
  });
});

//update history
socket.on("history_update", function(){
  location.reload();
});
