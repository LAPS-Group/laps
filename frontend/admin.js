import Vue from "vue";
import adminPanelModules from "./components/AdminPanelModules.vue";
import uploadMap from "./components/uploadMap.vue";
import login from "./components/login.vue";
import moduleUploader from "./components/moduleUploader.vue";
import registrationBox from "./components/registrationBox.vue";

new Vue({
  el: "#adminPanelModules",
  render: (a) => a(adminPanelModules),
});
new Vue({
  el: "#uploadMap",
  render: (b) => b(uploadMap),
});
new Vue({
  el: "#moduleUploader",
  render: (m) => m(moduleUploader),
});
new Vue({
  el: "#registrationBox",
  render: (r) => r(registrationBox),
});
