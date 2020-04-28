<template>
  <div id="map">
    <h2>Upload new map</h2>
    <label
      >GeoTiff file:
      <input type="file" ref="file" v-on:change="handleFileUpload()"
    /></label>
    <button v-on:click="submit()">Submit</button>
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
    };
  },

  methods: {
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
      axios
        .post(url, formData, {
          headers: {
            "Content-Type": "multipart/form-data",
          },
          withCredentials: true,
        })
        .then(function (res) {
          alert("Successfully uploaded map " + res.data);
        })
        .catch(function (err) {
          console.log(err);
          alert("Failed to upload map: " + err.data);
        });
    },
  },
};
</script>
