<template>
  <div>
    <h2>Module list</h2>
    <ul>
      <li v-for="module in modules">
        {{ module.name }} {{ module.version }} State:
        {{ getStateString(module) }},
        <a v-bind:href="moduleRoute(module, 'logs')">Logs</a>
        <button v-on:click="restartModule(module)">Restart</button
        ><button v-on:click="stopModule(module)">Stop</button>
      </li>
    </ul>
  </div>
</template>

<script>
import axios from "axios";
import { getRoute } from "route";
export default {
  data: function () {
    return {
      modules: [],
    };
  },
  beforeMount: async function () {
    this.refreshModules();
  },
  methods: {
    getStateString(module) {
      if (module.state === "other") {
        return module.message;
      } else {
        return module.state;
      }
    },
    refreshModules: async function () {
      let modules = await axios.get(getRoute("/module/all"), {
        withCredentials: true,
      });
      this.modules = modules.data;
    },
    moduleRoute: function (module, point) {
      return (
        getRoute("/module/") + module.name + "/" + module.version + "/" + point
      );
    },
    stopModule: function (module) {
      let url = this.moduleRoute(module, "stop");
      axios.post(url, { withCredentials: true }).catch(function (err) {
        alert("Failed to stop module: " + err);
      });
      this.refreshModules();
    },
    restartModule: function (module) {
      let url = this.moduleRoute(module, "restart");
      axios.post(url, { withCredentials: true }).catch(function (err) {
        alert("Failed to restart module: " + err);
      });
      this.refreshModules();
    },
  },
};
</script>
