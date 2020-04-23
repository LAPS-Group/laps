<template>
  <div id="sendCords">
    <!-- Creates 4 inputs field for coordinates, displays first 2 coordinates recived-->
    Start X <br /><input v-model="coordinates.start.x" /><br />
    Start Y <br /><input v-model="coordinates.start.y" /><br />
    End X <br /><input v-model="coordinates.stop.x" /> <br />
    End Y <br />
    <input v-model="coordinates.stop.y" /> <br />

    <!--Map Id <input v-model="map_id" > <br />-->

    <button v-on:click="submitPoints">Send</button>

    <!--Display the two first coordinates-->

    <template v-if="messageSent == true">
      <!--
      <p>

        Start: X:{{ coordinates.start.x }} Y:{{ coordinates.start.y }}
      </p>
      <!--
      <p>
        End: X:{{ coordinates.stop.x }} Y:{{ coordinates.stop.y }}
      </p>
    -->
      <!-- <request-job-results />-->
    </template>
  </div>
</template>
<script>
import axios from "axios";
//used to import data from other components
import { store, mutations } from "../store.js";
import { getRoute } from "route";
//import { getMap} from "GetMap.vue";

export default {
  components: {
    //RequestJobResults
  },
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
      //send map id
      //mutations.setmap_id(this.map_id);
      //getMap();
      //Convert inputs coords to ints
      this.coordinates.start.x = parseInt(this.coordinates.start.x);
      this.coordinates.start.y = parseInt(this.coordinates.start.y);
      this.coordinates.stop.x = parseInt(this.coordinates.stop.x);
      this.coordinates.stop.y = parseInt(this.coordinates.stop.y);
      //console.log(this.coordinates.stop.y);

      this.coordinates.map_id = store.map_id;
      this.coordinates.map_id = parseInt(this.coordinates.map_id);

      this.coordinates.algorithm.name = store.selected_algorithms.name;
      this.coordinates.algorithm.version = store.selected_algorithms.version;
      //convert coordinates to JSON
      let message = JSON.stringify(this.coordinates);
      console.log(message);
      //Send request
      let res = await axios.post(getRoute("/job"), message, {
        headers: {
          "Content-Type": "application/json",
        },
      });
      //Enables display of coordinates
      this.messageSent = true;
      this.job_token = res.data;
      mutations.setjob_token(this.job_token);
      this.send_job_token();
      //console.log(JSON.parse(JSON.stringify(this.job_token)));

      //console.log(JSON.parse(JSON.stringify(c)));

      //Sends update coordinates in store so they can be used by other components
      //mutations.setrecivedCoordinates(this.display.data);
    },
    send_job_token: async function () {
      try {
        const c = await axios.get(getRoute("/job/" + this.job_token));
        console.log("Job Done");
        console.log(JSON.parse(JSON.stringify(c.data)));
        mutations.setrecivedCoordinates(c.data);
      } catch (error) {
        console.log(error);
        if ((error = 504)) {
          console.log("504:timed out sending new request");
          this.send_job_token();
        }
        console.log("this is error");
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
