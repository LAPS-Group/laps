<template>
  <div id="moduleUploader">
    <h2>Upload new module</h2>
    <label
      >Module tape archive:
      <input
        type="file"
        ref="file"
        accept="application/x-tar"
        v-on:change="handleFileUpload()" /></label
    ><br />
    <label>Name: <input type="text" name="name" v-model="input.name" /></label
    ><br />
    <label
      >Version:
      <input type="text" name="version" v-model="input.version" /></label
    ><br />
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
      if (this.input.name == "" || this.input.version == "") {
        alert("Please input name and version");
        return;
      }
      if (this.file == null) {
        alert("Please select a file!");
        return;
      }

      let url = getRoute("/module");
      let formData = new FormData();
      formData.append("name", this.input.name);
      formData.append("version", this.input.version);

      //Need to set the content-type header on the module field, so recreate the file:
      console.log(this.file);
      let file = new File([this.file.slice()], "module.tar", {
        type: "application/x-tar",
      });
      formData.append("module", file);
      axios
        .post(url, formData, {
          headers: {
            "Content-Type": "multipart/form-data",
          },
          withCredentials: true,
        })
        .then(function (data) {
          alert("Successfully uploaded module!");
        })
        .catch(function (err) {
          alert("Failed to upload module: " + err.data);
        });
    },
  },
};
</script>
