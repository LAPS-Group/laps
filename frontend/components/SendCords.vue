<template>
  <div id="sendCords">
    <!-- Creates 4 inputs field for coordinates, displays first 2 coordinates recived-->
    Start X <input v-model="coordinates.start.x" /><br />
    Start Y <input v-model="coordinates.start.y" /><br />
    End X <input v-model="coordinates.end.x" /> <br />
    End Y <input v-model="coordinates.end.y" /> <br />
    <button v-on:click="submit_points">Send</button>
    <!--Display the two first coordinates, should either be rework to a loop or removed for its own component-->
    <template v-if="messageSent == true">
      <p>
        Start: X:{{ display.data.points[0].x }} Y:{{ display.data.points[0].y }}
      </p>

      <p>
        End: X:{{ display.data.points[1].x }} Y:{{ display.data.points[1].x }}
      </p>
    </template>
  </div>
</template>
<script>
import axios from "axios";
export default {
  data: function() {
    return {
      coordinates: {
        //coordinates to be sent
        start: { x: 1, y: 2 },
        end: { x: 2, y: 4 }
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
    submit_points: async function() {
      //convert coordinates to JSON
      let message = JSON.stringify(this.coordinates);
      //Send request
      let res = await axios.post("/job/submit", message, {
        headers: {
          "Content-Type": "application/json"
        }
      });
      //Enables display of coordinates
      this.messageSent = true;

      this.display = Object.assign({}, res);
    }
  }
};
</script>
