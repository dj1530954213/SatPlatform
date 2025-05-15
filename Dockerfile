# Stage 1: Build Angular Frontend
FROM node:20 AS frontend-builder

WORKDIR /app

# Copy package.json and package-lock.json (or yarn.lock)
COPY package.json package-lock.json* ./
# COPY yarn.lock ./ # Uncomment if you use Yarn

# Install frontend dependencies
RUN npm install
# RUN yarn install # Uncomment if you use Yarn

# Copy the rest of the frontend application files
COPY . .

# Build the Angular application
# Assuming your build script is 'ng build' or 'build' in package.json
# Adjust if your Angular project path or build command is different
# For example, if your Angular app is in a subfolder 'my-angular-app':
# WORKDIR /app/my-angular-app
# RUN npm run build --prod
RUN npm run build -- --configuration production


# Stage 2: Build Tauri Application with Rust
FROM rust:1.78 AS tauri-builder

# Install necessary Linux dependencies for Tauri
RUN apt-get update && \
    apt-get install -y \
    libwebkit2gtk-4.0-dev \
    build-essential \
    curl \
    wget \
    file \
    libssl-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    patchelf \
    && rm -rf /var/lib/apt/lists/*

# Install a specific version of tauri-cli if needed, otherwise latest
# RUN cargo install tauri-cli --version "^2.0.0" # Match your project's tauri-cli version
RUN cargo install tauri-cli

WORKDIR /app

# Copy frontend build artifacts from the frontend-builder stage
# Adjust 'dist/your-angular-app-name' to your actual Angular output directory
# Typically, Angular outputs to 'dist/<project-name>'
COPY --from=frontend-builder /app/dist /app/dist
# If your tauri.conf.json expects frontend files at a different location (e.g. "public" or "dist"),
# ensure this COPY command places them correctly relative to src-tauri, or adjust tauri.conf.json build.distDir.
# For example, if distDir is "../dist", and your Angular output is in /app/dist/my-app,
# and your tauri project is in /app/src-tauri:
# COPY --from=frontend-builder /app/dist/my-app /app/dist # assuming tauri.conf.json\'s build.distDir is "../dist"

# Copy Cargo.toml, Cargo.lock, and tauri.conf.json first to leverage Docker cache
COPY src-tauri/Cargo.toml src-tauri/Cargo.lock ./src-tauri/
COPY tauri.conf.json .

# Create a dummy main.rs to cache dependencies if Cargo.toml/Cargo.lock haven't changed
RUN mkdir -p src-tauri/src && \
    echo "fn main() {}" > src-tauri/src/main.rs
# Build dependencies. This will cache them if Cargo.toml and Cargo.lock don't change.
# Ensure to specify the target if you are cross-compiling or want a specific target for caching
RUN cd src-tauri && cargo build --release --target x86_64-unknown-linux-gnu

# Copy the rest of the application source code
COPY src-tauri ./src-tauri
# Copy the rest of the project files if needed (e.g. root package.json for tauri-cli context)
COPY package.json .

# Build the Tauri application for Linux
# The --target might be optional if your Rust default target is already x86_64-unknown-linux-gnu
# However, explicitly setting it is safer for CI/CD environments.
RUN cd src-tauri && cargo tauri build --release --target x86_64-unknown-linux-gnu

# The final artifacts will be in /app/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/
# Typically an .AppImage or .deb file.
# For example: /app/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/appimage/your-app-name_0.1.0_amd64.AppImage

CMD ["echo", "Build complete. Artifacts are in /app/src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/"] 