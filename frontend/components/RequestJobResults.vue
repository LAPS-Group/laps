<template>
  <div id="RequestJobResults"></div>
</template>

<script>
import axios from "axios";
import { store, mutations } from "../store.js";
import { getRoute } from "route";

export default {
  data: function() {
    return {};
  },
  mounted: async function() {
    store.recivedCoordinates = await axios.post(
      getRoute("/job/" + store.job_token)
    );
    console.log(JSON.parse(JSON.stringify(this.job_token)));
  }
};
</script>
