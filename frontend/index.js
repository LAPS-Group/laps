import axios from "axios";
import Vue from "vue";

var app = new Vue({
  el: "#root",
  data: {
    coordinates: {
      start: {
        x: 1,
        y: 2
      },
      end: {
        x: 2,
        y: 4
      }
    }
  },
  methods: {
    submit_points: function() {
      let message = JSON.stringify(self.coordinates);
      console.log(message);
      axios.post("/job/submit", message, {
        headers: {
          "Content-Type": "application/json"
        }
      });
    }
  }
});
