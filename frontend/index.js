import Vue from "vue";
//import axios from 'axios';
//import App from './components/App.vue';
import SendCords from "./components/SendCords.vue";
import getMap from "./components/GetMap.vue";
import getAlgorithm from "./components/GetAlgorithm.vue";
import header from "./components/Header.vue";
//import adminPanelModules from "./components/AdminPanelModules.vue";
//import laps_logo from "./LAPS1.png";

//Calls send app, its sends coordinates
new Vue({
  el: "#sendCords",
  render: (s) => s(SendCords),
});
new Vue({
  el: "#getMap",
  render: (g) => g(getMap),
});
new Vue({
  el: "#getAlgorithm",
  render: (a) => a(getAlgorithm),
});
new Vue({
  el: "#header",
  render: (h) => h(header),
});
