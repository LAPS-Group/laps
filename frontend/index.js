import Vue from "vue";
//import axios from 'axios';
//import App from './components/App.vue';
import SendCords from "./components/SendCords.vue";
import getMap from "./components/GetMap.vue";
//import DrawCords from './components/DrawCords.vue';

//Calls send app, its sends coordinates
new Vue({
  el: "#sendCords",
  render: s => s(SendCords)
});

new Vue({
  el: "#getMap",
  render: g => g(getMap)
});
