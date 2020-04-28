const HtmlWebpackPlugin = require("html-webpack-plugin");
const CopyWebpackPlugin = require("copy-webpack-plugin");
const path = require("path");
const VueLoaderPlugin = require("vue-loader/lib/plugin");

module.exports = (env) => {
  let routeAlias;
  if (env.production) {
    console.log("Running in production mode");
    routeAlias = "./frontend/route_production.js";
  } else {
    console.log("Not using production");
    routeAlias = "./frontend/route.js";
  }

  return {
    entry: {
      index: "./frontend/index.js",
      admin: "./frontend/admin.js",
      login: "./frontend/login.js",
    },
    output: {
      filename: "[name].js",
      path: path.resolve(__dirname, "dist"),
    },
    devServer: {
      disableHostCheck: true,
      headers: {
        "Access-Control-Allow-Origin": "*",
        "Access-Control-Allow-Credentials": "true",
        "Access-Control-Allow-Methods":
          "GET, POST, PUT, DELETE, PATCH, OPTIONS",
        "Access-Control-Allow-Headers":
          "X-Requested-With, content-type, Authorization",
      },
    },
    resolve: {
      alias: {
        vue$: "vue/dist/vue.esm.js",
        route: path.resolve(__dirname, routeAlias),
        images: path.resolve(__dirname, "frontend/images"),
      },
    },
    module: {
      rules: [
        // ... other rules
        {
          test: /\.vue$/,
          loader: "vue-loader",
        },
        {
          test: /\.css$/,
          use: ["vue-style-loader", "css-loader"],
        },
        {
          test: /\.(png|jpe?g|gif)$/i,
          loader: "file-loader",
          options: {
            outputPath: "images",
            publicPath: "images",
            emitFile: true,
          },
        },
        {
          test: /\.scss$/,
          use: ["vue-style-loader", "css-loader", "sass-loader"],
        },
      ],
    },
    plugins: [
      new VueLoaderPlugin(),
      // new CopyWebpackPlugin([
      //   { from: "frontend/*.html", flatten: true },
      // ]),
      new HtmlWebpackPlugin({
        filename: "index.html",
        template: "frontend/index.html",
        inject: false,
        chunks: ["index"],
      }),
      new HtmlWebpackPlugin({
        filename: "admin.html",
        template: "frontend/admin.html",
        inject: false,
        chunks: ["admin"],
      }),
      new HtmlWebpackPlugin({
        filename: "login.html",
        template: "frontend/login.html",
        inject: false,
        chunks: ["login"],
      }),
    ],
  };
};
