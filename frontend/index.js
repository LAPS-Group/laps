import axios from "axios";
import Vue from "vue";
import SendCords from "./components/SendCords.vue";
import App from "./components/App.vue";

//test app that displays hello world
new Vue({
  el: "#app",
  render: h => h(App)
});
//Calls send app, its sends coordinates
new Vue({
  el: "#sendCords",
  render: s => s(SendCords)
});
