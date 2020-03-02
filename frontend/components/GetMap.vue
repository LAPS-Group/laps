<template>
  <div id="getMap">
    <!-- add an input field and adds it vue reactive elements-->
    Request map by ID<input v-model="map_id" />
    <button v-on:click="fetchMap">Get Map</button>
    
    <!-- creates a new template wich is only displayed if a map is recived-->

    <template v-if="pictureRecived == true"
      ><br />

      <div class="map">
        <div class="mapcontainer">
          <img src="http://localhost:8000/maps/1" />
        </div>
        <!-- Calls the component DrawCords-->
        <draw-cordinates />
      </div>

    </template>
  </div>
</template>
<script>
import DrawCordinates from "./DrawCords.vue";
import axios from "axios";

export default {
  //defines components used
  components: {
    DrawCordinates
  },

  data: function() {
    //defines variables in vue reactive element.
    return {
      pictureRecived: false,
      map: null,
      map_id: null,
      map_path: "/maps/",
      colour: "red",
      x: 50,
      y: 45,
      DrawCordsTest: 1,
      user: {
        username: "noe"
      }
    };
  },
  methods: {
    //Fetch map by user ID
    fetchMap: async function() {
      this.map = await axios.get(this.map_path + this.map_id);

      //Sets that a map is recived and the render can be rendered
      this.pictureRecived = true;
    }
  }
};
</script>
<style>
#getMap {
  font-size: 18px;
  font-family: "Roboto", sans-serif;
}
canvas {
  align-items: flex-start;
  position: absolute;
  top: 0px;
  left: 0px;

  background-color: red;
  z-index: 1;
}
.mapcontainer {
  position: absolute;
  align-items: flex-start;
}
.map {
  align-items: flex-start;
  position: relative;
}
</style>
