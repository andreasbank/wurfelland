// main.cpp — Rotating cube demo + FPS + first-person camera
// Uses GLFW window user pointer to pass state into callbacks (no globals needed).

#include <glad/glad.h>
#include <GLFW/glfw3.h>

#include <glm/glm.hpp>
#include <glm/gtc/matrix_transform.hpp>
#include <glm/gtc/type_ptr.hpp>

#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

// ---------- Shaders ----------
static const char* kVert = R"(
#version 330 core
layout (location = 0) in vec3 aPos;
layout (location = 1) in vec3 aNormal;
layout (location = 2) in vec2 aUV;

uniform mat4 uMVP;
uniform mat4 uModel;
uniform vec3 uLightDir;

out vec2 vUV;
out float vLight;

void main() {
    gl_Position = uMVP * vec4(aPos, 1.0);
    vUV = aUV;

    vec3 N = normalize(mat3(uModel) * aNormal);
    float ndl = max(dot(N, -normalize(uLightDir)), 0.2);
    vLight = ndl;
}
)";

static const char* kFrag = R"(
#version 330 core
in vec2 vUV;
in float vLight;

uniform sampler2D uTex;

out vec4 FragColor;

void main() {
    vec3 tex = texture(uTex, vUV).rgb;
    FragColor = vec4(tex * vLight, 1.0);
}
)";

// ---------- Helpers ----------
static void die(const char* msg) {
    std::fprintf(stderr, "Error: %s\n", msg);
    std::exit(EXIT_FAILURE);
}
static GLuint compileShader(GLenum type, const char* src) {
    GLuint s = glCreateShader(type);
    glShaderSource(s, 1, &src, nullptr);
    glCompileShader(s);
    GLint ok = 0;
    glGetShaderiv(s, GL_COMPILE_STATUS, &ok);
    if (!ok) {
        char log[2048];
        glGetShaderInfoLog(s, sizeof(log), nullptr, log);
        std::fprintf(stderr, "Shader compile error:\n%s\n", log);
        std::exit(EXIT_FAILURE);
    }
    return s;
}
static GLuint makeProgram(const char* vs, const char* fs) {
    GLuint v = compileShader(GL_VERTEX_SHADER, vs);
    GLuint f = compileShader(GL_FRAGMENT_SHADER, fs);
    GLuint p = glCreateProgram();
    glAttachShader(p, v);
    glAttachShader(p, f);
    glLinkProgram(p);
    glDeleteShader(v);
    glDeleteShader(f);
    GLint ok = 0;
    glGetProgramiv(p, GL_LINK_STATUS, &ok);
    if (!ok) {
        char log[2048];
        glGetProgramInfoLog(p, sizeof(log), nullptr, log);
        std::fprintf(stderr, "Program link error:\n%s\n", log);
        std::exit(EXIT_FAILURE);
    }
    return p;
}

// ---------- Per-window state ----------
struct CameraState {
    // pose
    glm::vec3 pos{0.f, 0.f, 3.f};
    float yaw   = -90.0f;
    float pitch =   0.0f;

    // mouse tracking
    bool   firstMouse = true;
    double lastX = 0.0, lastY = 0.0;
    float  sensitivity = 0.1f;
};

struct AppState {
    CameraState cam;
    int fbw = 1280, fbh = 720;
};

// ---------- Callbacks (free functions) ----------
static void FramebufferSizeCallback(GLFWwindow* window, int w, int h) {
    auto* app = static_cast<AppState*>(glfwGetWindowUserPointer(window));
    if (!app) return;
    app->fbw = (w > 0 ? w : 1);
    app->fbh = (h > 0 ? h : 1);
    glViewport(0, 0, app->fbw, app->fbh);
}

static void CursorPosCallback(GLFWwindow* window, double xpos, double ypos) {
    auto* app = static_cast<AppState*>(glfwGetWindowUserPointer(window));
    if (!app) return;
    auto& cam = app->cam;

    if (cam.firstMouse) {
        cam.firstMouse = false;
        cam.lastX = xpos; cam.lastY = ypos;
        return;
    }

    double dx = xpos - cam.lastX;
    double dy = cam.lastY - ypos; // invert Y
    cam.lastX = xpos; cam.lastY = ypos;

    cam.yaw   += float(dx) * cam.sensitivity;
    cam.pitch += float(dy) * cam.sensitivity;
    if (cam.pitch >  89.f) cam.pitch =  89.f;
    if (cam.pitch < -89.f) cam.pitch = -89.f;
}

// ---------- Main ----------
int main() {
    if (!glfwInit()) die("glfwInit failed");
    glfwWindowHint(GLFW_CONTEXT_VERSION_MAJOR, 3);
    glfwWindowHint(GLFW_CONTEXT_VERSION_MINOR, 3);
    glfwWindowHint(GLFW_OPENGL_PROFILE, GLFW_OPENGL_CORE_PROFILE);

    const int winW = 1280, winH = 720;
    GLFWwindow* win = glfwCreateWindow(winW, winH, "Wurfelland - Boot", nullptr, nullptr);
    if (!win) die("glfwCreateWindow failed");
    glfwMakeContextCurrent(win);

    if (!gladLoadGLLoader((GLADloadproc)glfwGetProcAddress)) die("GLAD init failed");
    glfwSwapInterval(1); // vsync

    // state
    AppState app{};
    app.fbw = winW; app.fbh = winH;
    glfwSetWindowUserPointer(win, &app);
    glfwSetFramebufferSizeCallback(win, FramebufferSizeCallback);
    glfwSetCursorPosCallback(win,   CursorPosCallback);
    glfwSetInputMode(win, GLFW_CURSOR, GLFW_CURSOR_DISABLED);

    // GL setup
    glEnable(GL_DEPTH_TEST);
    glEnable(GL_CULL_FACE);
    glCullFace(GL_BACK);

    GLuint prog = makeProgram(kVert, kFrag);
    glUseProgram(prog);

    // Cube geometry
    struct V { glm::vec3 p, n; glm::vec2 uv; };
    std::vector<V> verts;
    std::vector<unsigned> idx;

    auto addFace = [&](glm::vec3 a, glm::vec3 b, glm::vec3 c, glm::vec3 d, glm::vec3 n){
        unsigned base = (unsigned)verts.size();
        verts.push_back({a,n,{0,0}});
        verts.push_back({b,n,{1,0}});
        verts.push_back({c,n,{1,1}});
        verts.push_back({d,n,{0,1}});
        idx.push_back(base+0); idx.push_back(base+1); idx.push_back(base+2);
        idx.push_back(base+0); idx.push_back(base+2); idx.push_back(base+3);
    };

    float s = 0.5f;
    glm::vec3 p000{-s,-s,-s}, p001{-s,-s, s}, p010{-s, s,-s}, p011{-s, s, s};
    glm::vec3 p100{ s,-s,-s}, p101{ s,-s, s}, p110{ s, s,-s}, p111{ s, s, s};

    addFace(p100,p101,p111,p110, {+1,0,0}); // +X
    addFace(p001,p000,p010,p011, {-1,0,0}); // -X
    addFace(p010,p110,p111,p011, {0,+1,0}); // +Y
    addFace(p000,p001,p101,p100, {0,-1,0}); // -Y
    addFace(p101,p001,p011,p111, {0,0,+1}); // +Z
    addFace(p000,p100,p110,p010, {0,0,-1}); // -Z

    GLuint vao, vbo, ebo;
    glGenVertexArrays(1,&vao);
    glGenBuffers(1,&vbo);
    glGenBuffers(1,&ebo);

    glBindVertexArray(vao);
    glBindBuffer(GL_ARRAY_BUFFER, vbo);
    glBufferData(GL_ARRAY_BUFFER, verts.size()*sizeof(V), verts.data(), GL_STATIC_DRAW);
    glBindBuffer(GL_ELEMENT_ARRAY_BUFFER, ebo);
    glBufferData(GL_ELEMENT_ARRAY_BUFFER, idx.size()*sizeof(unsigned), idx.data(), GL_STATIC_DRAW);

    glVertexAttribPointer(0,3,GL_FLOAT,GL_FALSE,sizeof(V),(void*)offsetof(V,p));
    glEnableVertexAttribArray(0);
    glVertexAttribPointer(1,3,GL_FLOAT,GL_FALSE,sizeof(V),(void*)offsetof(V,n));
    glEnableVertexAttribArray(1);
    glVertexAttribPointer(2,2,GL_FLOAT,GL_FALSE,sizeof(V),(void*)offsetof(V,uv));
    glEnableVertexAttribArray(2);

    // Checkerboard texture (no files needed)
    const int TW=64, TH=64;
    std::vector<unsigned char> tex(TW*TH*3);
    for(int y=0;y<TH;++y){
        for(int x=0;x<TW;++x){
            int c = ((x/8)+(y/8)) & 1;
            unsigned char v = c ? 220 : 70;
            tex[(y*TW+x)*3+0]=v; tex[(y*TW+x)*3+1]=v; tex[(y*TW+x)*3+2]=v;
        }
    }
    GLuint texId;
    glGenTextures(1,&texId);
    glBindTexture(GL_TEXTURE_2D, texId);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGB8, TW, TH, 0, GL_RGB, GL_UNSIGNED_BYTE, tex.data());
    glGenerateMipmap(GL_TEXTURE_2D);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR_MIPMAP_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);

    // Uniforms
    GLint uMVP      = glGetUniformLocation(prog, "uMVP");
    GLint uModel    = glGetUniformLocation(prog, "uModel");
    GLint uLightDir = glGetUniformLocation(prog, "uLightDir");
    GLint uTex      = glGetUniformLocation(prog, "uTex");
    glUniform1i(uTex, 0);

    // Timing / FPS
    auto lastTime = std::chrono::high_resolution_clock::now();
    double accTitle = 0.0;
    int frames = 0;

    while(!glfwWindowShouldClose(win)){
        // dt
        auto now = std::chrono::high_resolution_clock::now();
        float dt = std::chrono::duration<float>(now - lastTime).count();
        lastTime = now;

        // input (WASD + Space/Shift)
        glm::vec3 front{
            std::cos(glm::radians(app.cam.yaw)) * std::cos(glm::radians(app.cam.pitch)),
            std::sin(glm::radians(app.cam.pitch)),
            std::sin(glm::radians(app.cam.yaw)) * std::cos(glm::radians(app.cam.pitch))
        };
        glm::vec3 fwd   = glm::normalize(front);
        glm::vec3 right = glm::normalize(glm::cross(fwd, {0,1,0}));

        const float move = 4.5f * dt;
        if (glfwGetKey(win, GLFW_KEY_W) == GLFW_PRESS) app.cam.pos += fwd   * move;
        if (glfwGetKey(win, GLFW_KEY_S) == GLFW_PRESS) app.cam.pos -= fwd   * move;
        if (glfwGetKey(win, GLFW_KEY_A) == GLFW_PRESS) app.cam.pos -= right * move;
        if (glfwGetKey(win, GLFW_KEY_D) == GLFW_PRESS) app.cam.pos += right * move;
        if (glfwGetKey(win, GLFW_KEY_SPACE) == GLFW_PRESS)      app.cam.pos.y += move;
        if (glfwGetKey(win, GLFW_KEY_LEFT_SHIFT) == GLFW_PRESS) app.cam.pos.y -= move;
        if (glfwGetKey(win, GLFW_KEY_ESCAPE) == GLFW_PRESS) glfwSetWindowShouldClose(win, true);

        // render
        glViewport(0,0,app.fbw,app.fbh);
        glClearColor(0.15f, 0.18f, 0.23f, 1.0f);
        glClear(GL_COLOR_BUFFER_BIT|GL_DEPTH_BUFFER_BIT);

        glm::mat4 proj = glm::perspective(glm::radians(60.0f), (float)app.fbw/(float)app.fbh, 0.1f, 1000.0f);
        glm::mat4 view = glm::lookAt(app.cam.pos, app.cam.pos + fwd, {0,1,0});

        float t = (float)glfwGetTime();
        glm::mat4 model(1.0f);
        model = glm::rotate(model, t*0.7f, {0,1,0});
        model = glm::rotate(model, t*0.31f, {1,0,0});
        glm::mat4 mvp = proj * view * model;

        glUseProgram(prog);
        glUniformMatrix4fv(uMVP,   1, GL_FALSE, glm::value_ptr(mvp));
        glUniformMatrix4fv(uModel, 1, GL_FALSE, glm::value_ptr(model));
        glUniform3f(uLightDir, -0.6f, -1.0f, -0.2f);

        glActiveTexture(GL_TEXTURE0);
        glBindTexture(GL_TEXTURE_2D, texId);

        glBindVertexArray(vao);
        glDrawElements(GL_TRIANGLES, (GLsizei)idx.size(), GL_UNSIGNED_INT, 0);

        glfwSwapBuffers(win);
        glfwPollEvents();

        // fps in title
        frames++;
        accTitle += dt;
        if (accTitle >= 1.0) {
            char title[128];
            std::snprintf(title, sizeof(title), "Wurfelland - Boot  |  FPS: %d  |  dt: %.2f ms",
                          frames, 1000.0/frames);
            glfwSetWindowTitle(win, title);
            frames = 0; accTitle = 0.0;
        }
    }

    glfwTerminate();
    return 0;
}