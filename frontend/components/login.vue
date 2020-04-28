<template>
  <div id="login">
    <input
      type="text"
      name="username"
      v-model="input.username"
      placeholder="Username"
    />
    <input
      type="password"
      name="password"
      v-model="input.password"
      placeholder="Password"
    />
    <button type="button" v-on:click="login()">Login</button>
  </div>
</template>

<script>
import { getRoute } from "route";
import axios from "axios";

export default {
  data() {
    return {
      input: {
        username: "",
        password: "",
      },
    };
  },

  methods: {
    login: async function () {
      if (this.input.username !== "" && this.input.password != "") {
        let url = getRoute("/login");
        const params = new URLSearchParams();
        params.append("username", this.input.username);
        params.append("password", this.input.password);
        axios
          .post(url, params)
          .then(function (data) {
            window.location.href = "admin";
          })
          .catch(function (err) {
            alert("Login failed " + err);
          });
      } else {
        alert("Please type in your username and password.");
      }
    },
  },
};
</script>
