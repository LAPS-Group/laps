import Vue from "vue";
import sendCords from "./components/SendCords.vue";
import getMap from "./components/GetMap.vue";
import getAlgorithm from "./components/GetAlgorithm.vue";
import header from "./components/Header.vue";
import loginLink from "./components/loginLink.vue";

//Calls send app, its sends coordinates
new Vue({
  el: "#sendCords",
  render: (s) => s(sendCords),
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
new Vue({
  el: "#loginLink",
  render: (l) => l(loginLink),
});
