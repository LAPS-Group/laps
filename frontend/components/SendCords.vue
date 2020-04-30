<!-- this is file is used to take input from the user and format it correctly and send to the backend, algorithm and map is gotten from different file, but is add to the request here.-->
<template>
  <div id="sendCords">
    <!-- Creates 4 inputs field for coordinates, displays first 2 coordinates recived-->

    Start X <br /><input v-model="coordinates.start.x" /><br />
    Start Y <br /><input v-model="coordinates.start.y" /><br />
    End X <br /><input v-model="coordinates.stop.x" /> <br />
    End Y <br /><input v-model="coordinates.stop.y" /> <br />

    <button v-on:click="submitPoints">Send</button>
  </div>
</template>
<script>
import axios from "axios";
//used to import data from other components
import { store, mutations } from "../store.js";
import { getRoute } from "route";

export default {
  computed: {
    tester() {
      return store.tester;
    },
  },
  data: function () {
    return {
      coordinates: {
        //coordinates to be sent
        start: { x: null, y: null },
        stop: { x: null, y: null },
        map_id: null,
        algorithm: {
          name: null,
          version: null,
        },
      },
      job_token: {},
      display: {
        data: {
          points: [],
        },
      },
      messageSent: false,
      map_id: null,
    };
  },

  computed: {
    selected_algorithms() {
      return store.selected_algorithms;
    },
  },
  methods: {
    submitPoints: async function () {
      //Convert inputs coords to ints
      this.coordinates.start.x = parseInt(this.coordinates.start.x);
      this.coordinates.start.y = parseInt(this.coordinates.start.y);
      this.coordinates.stop.x = parseInt(this.coordinates.stop.x);
      this.coordinates.stop.y = parseInt(this.coordinates.stop.y);

      // Gets the map from the store and converts it into an int
      this.coordinates.map_id = store.map_id;
      this.coordinates.map_id = parseInt(this.coordinates.map_id);
      // Gets the currently selected algorithm from the store
      this.coordinates.algorithm.name = store.selected_algorithms.name;
      this.coordinates.algorithm.version = store.selected_algorithms.version;

      //convert coordinates to JSON
      let message = JSON.stringify(this.coordinates);
      console.log(message);

      //Start the job based on sent information and returns id to fetch result when done
      let res = await axios.post(getRoute("/job"), message, {
        headers: {
          "Content-Type": "application/json",
        },
      });

      //Stores job token in store
      this.job_token = res.data;
      mutations.setjob_token(this.job_token);

      // Send the jobtoken, if the job is done return the result of the job, if not send a new request when the last times out.
      this.getJobResult();
    },
    getJobResult: async function () {
      try {
        const c = await axios.get(getRoute("/job/" + this.job_token));
        console.log("Job Done");
        mutations.setrecivedCoordinates(c.data);
      } catch (error) {
        console.log(error);
        //If the error is a time out send a new request
        if ((error = 504)) {
          console.log("504:timed out sending new request");
          this.getJobResult();
        }
      }
    },
  },
};
</script>
<style>
#sendCords {
  font-size: 18px;
  font-family: "Roboto", sans-serif;
}
</style>
