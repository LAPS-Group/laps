import Vue from "vue";
import login from "./components/login.vue";
new Vue({
  el: "#login",
  render: (c) => c(login),
});
