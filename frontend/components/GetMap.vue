<template>
  <div id="getMap">
    <!-- add an input field and adds it vue reactive elements-->
    <br />
    Select Map ID
    <br />
    <input v-model="map_id" @change="getMap" />
    <br />
    <!-- calls a function to display the map-->
    <button v-on:click="getMap">Get Map</button>

    <!-- creates a new template wich is only displayed if a map is recived-->

    <template v-if="pictureRecived == true"
      ><br />

      <div class="map">
        <div class="mapcontainer">
          <img :src="this.map_link" />
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
import { getRoute } from "route";
import { store, mutations } from "../store.js";

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
      map_path: "/map/",

      map_link: ""
    };
  },

  methods: {
    // displays the map based on ID
    getMap: function() {
      this.map_link = getRoute(this.map_path + this.map_id);

      // Is set to true so that, it doesn't try display maps without a map
      this.pictureRecived = true;
      // send map ID to the store
      mutations.setmap_id(this.map_id);
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
  float: left;
  left: 300px;
}
</style>
