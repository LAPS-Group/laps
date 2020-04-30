<template>
  <div id="map">
    <h2>Upload new map</h2>
    <label
      >GeoTiff file:
      <input
        type="file"
        ref="file"
        accept="image/tiff"
        v-on:change="handleFileUpload()"
    /></label>
    <button v-on:click="submit()">Submit</button>
    <h2>Delete a map</h2>
    <ul>
      <li v-for="map in maps">
        ID: {{ map }} <button v-on:click="deleteMap(map)">Delete</button>
      </li>
    </ul>
  </div>
</template>

<script>
import { getRoute } from "route";
import axios from "axios";

export default {
  data() {
    return {
      file: null,
      input: {
        name: "",
        version: "",
      },
      maps: [],
    };
  },
  beforeMount: async function () {
    await this.refreshMaps();
  },
  methods: {
    refreshMaps: async function () {
      let maps = await axios.get(getRoute("/maps"));
      this.maps = maps.data.maps;
      this.maps.sort(function (a, b) {
        return parseInt(a) - parseInt(b);
      });
    },
    deleteMap: async function (map) {
      let url = getRoute("/map/" + map);
      try {
        await axios.delete(url, {
          withCredentials: true,
        });
        await this.refreshMaps();
      } catch (err) {
        console.log(err);
        alert("Failed to delete map: " + err.response.data);
      }
    },
    handleFileUpload() {
      this.file = this.$refs.file.files[0];
    },
    submit: async function () {
      if (this.file == null) {
        alert("Please select a file!");
        return;
      }
      let url = getRoute("/map");
      let formData = new FormData();
      formData.append("data", this.file);
      try {
        await axios.post(url, formData, {
          headers: {
            "Content-Type": "multipart/form-data",
          },
          withCredentials: true,
        });
        await this.refreshMaps();
      } catch (err) {
        console.log(err);
        alert("Failed to upload map: " + err.response);
      }
    },
  },
};
</script>
