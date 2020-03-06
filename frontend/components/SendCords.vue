<template>
  <div id="sendCords">
    <!-- Creates 4 inputs field for coordinates, displays first 2 coordinates recived-->
    Start X <input v-model="coordinates.start.x" /><br />
    Start Y <input v-model="coordinates.start.y" /><br />
    End X <input v-model="coordinates.end.x" /> <br />
    End Y <input v-model="coordinates.end.y" /> <br />
    <button v-on:click="submitPoints">Send</button>
    <!--Display the two first coordinates-->
    <template v-if="messageSent == true">
      <p>
        Start: X:{{ display.data.points[0].x }} Y:{{ display.data.points[0].y }}
      </p>
      <!--
      <p>
        End: X:{{ display.data.points[1].x }} Y:{{ display.data.points[1].x }}
      </p>
      -->
    </template>
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
    }
  },
  data: function() {
    return {
      coordinates: {
        //coordinates to be sent
        start: { x: 1, y: 2 },
        end: { x: 20, y: 40 }
      },
      display: {
        data: {
          points: []
        }
      },
      messageSent: false
    };
  },
  methods: {
    submitPoints: async function() {
      //convert coordinates to JSON
      let message = JSON.stringify(this.coordinates);
      //Send request
      let res = await axios.post(getRoute("/job/submit"), message, {
        headers: {
          "Content-Type": "application/json"
        }
      });
      //Enables display of coordinates
      this.messageSent = true;

      this.display = Object.assign({}, res);
      //Sends update coordinates in store so they can be used by other components
      mutations.setrecivedCoordinates(this.display.data);
    }
  }
};
</script>
<style>
#sendCords {
  font-size: 18px;
  font-family: "Roboto", sans-serif;
}
</style>
