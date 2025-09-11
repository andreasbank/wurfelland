// main.cpp — Rotating cube + FPS + orthographic "Wufferland" text (≤5% height)
// Uses GLFW window user pointer for state; projections cached & recomputed on resize.

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
#include <unordered_map>
#include <algorithm>

#include "voxel.hpp"

// ---------- Shaders ----------
// 3D shader
static const char* kVert3D = R"(
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

static const char* kFrag3D = R"(
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

// 2D HUD text shader
static const char* kVertHUD = R"(
#version 330 core
layout (location = 0) in vec2 aPos;  // pixel coords
layout (location = 1) in vec2 aUV;

uniform mat4 uProj; // orthographic

out vec2 vUV;

void main() {
    gl_Position = uProj * vec4(aPos, 0.0, 1.0);
    vUV = aUV;
}
)";

static const char* kFragHUD = R"(
#version 330 core
in vec2 vUV;
uniform sampler2D uFont;
uniform vec3 uColor;

out vec4 FragColor;

void main() {
    float a = texture(uFont, vUV).r; // sample alpha from red channel
    FragColor = vec4(uColor, a);
}
)";

// ---------- Helpers ----------
static void die(const char* msg) { std::fprintf(stderr, "Error: %s\n", msg); std::exit(EXIT_FAILURE); }

static GLuint compileShader(GLenum type, const char* src) {
    GLuint s = glCreateShader(type);
    glShaderSource(s, 1, &src, nullptr);
    glCompileShader(s);
    GLint ok = 0; glGetShaderiv(s, GL_COMPILE_STATUS, &ok);
    if (!ok) {
        char log[2048]; glGetShaderInfoLog(s, sizeof(log), nullptr, log);
        std::fprintf(stderr, "Shader compile error:\n%s\n", log);
        std::exit(EXIT_FAILURE);
    }
    return s;
}
static GLuint makeProgram(const char* vs, const char* fs) {
    GLuint v = compileShader(GL_VERTEX_SHADER, vs);
    GLuint f = compileShader(GL_FRAGMENT_SHADER, fs);
    GLuint p = glCreateProgram();
    glAttachShader(p, v); glAttachShader(p, f); glLinkProgram(p);
    glDeleteShader(v); glDeleteShader(f);
    GLint ok = 0; glGetProgramiv(p, GL_LINK_STATUS, &ok);
    if (!ok) {
        char log[2048]; glGetProgramInfoLog(p, sizeof(log), nullptr, log);
        std::fprintf(stderr, "Program link error:\n%s\n", log);
        std::exit(EXIT_FAILURE);
    }
    return p;
}

// ---------- Minimal 5x7 bitmap font for "Wufferland" ----------
struct Glyph { unsigned char col[5]; }; // 5 columns × 7 rows; bit0 = top row

// Classic 5x7 ASCII font (characters 32 ' ' to 126 '~')
static const std::unordered_map<char, Glyph> kFont = {
    // Space
    { ' ', {{0x00,0x00,0x00,0x00,0x00}} },
    { '!', {{0x00,0x00,0x5F,0x00,0x00}} },
    { '"', {{0x00,0x07,0x00,0x07,0x00}} },
    { '#', {{0x14,0x7F,0x14,0x7F,0x14}} },
    { '$', {{0x24,0x2A,0x7F,0x2A,0x12}} },
    { '%', {{0x23,0x13,0x08,0x64,0x62}} },
    { '&', {{0x36,0x49,0x55,0x22,0x50}} },
    { '\'',{{0x00,0x05,0x03,0x00,0x00}} },
    { '(', {{0x00,0x1C,0x22,0x41,0x00}} },
    { ')', {{0x00,0x41,0x22,0x1C,0x00}} },
    { '*', {{0x14,0x08,0x3E,0x08,0x14}} },
    { '+', {{0x08,0x08,0x3E,0x08,0x08}} },
    { ',', {{0x00,0x50,0x30,0x00,0x00}} },
    { '-', {{0x08,0x08,0x08,0x08,0x08}} },
    { '.', {{0x00,0x60,0x60,0x00,0x00}} },
    { '/', {{0x20,0x10,0x08,0x04,0x02}} },

    // Digits
    { '0', {{0x3E,0x51,0x49,0x45,0x3E}} },
    { '1', {{0x00,0x42,0x7F,0x40,0x00}} },
    { '2', {{0x42,0x61,0x51,0x49,0x46}} },
    { '3', {{0x21,0x41,0x45,0x4B,0x31}} },
    { '4', {{0x18,0x14,0x12,0x7F,0x10}} },
    { '5', {{0x27,0x45,0x45,0x45,0x39}} },
    { '6', {{0x3C,0x4A,0x49,0x49,0x30}} },
    { '7', {{0x01,0x71,0x09,0x05,0x03}} },
    { '8', {{0x36,0x49,0x49,0x49,0x36}} },
    { '9', {{0x06,0x49,0x49,0x29,0x1E}} },

    // Punctuation
    { ':', {{0x00,0x36,0x36,0x00,0x00}} },
    { ';', {{0x00,0x56,0x36,0x00,0x00}} },
    { '<', {{0x08,0x14,0x22,0x41,0x00}} },
    { '=', {{0x14,0x14,0x14,0x14,0x14}} },
    { '>', {{0x00,0x41,0x22,0x14,0x08}} },
    { '?', {{0x02,0x01,0x51,0x09,0x06}} },
    { '@', {{0x32,0x49,0x79,0x41,0x3E}} },

    // Uppercase A–Z
    { 'A', {{0x7E,0x11,0x11,0x11,0x7E}} },
    { 'B', {{0x7F,0x49,0x49,0x49,0x36}} },
    { 'C', {{0x3E,0x41,0x41,0x41,0x22}} },
    { 'D', {{0x7F,0x41,0x41,0x22,0x1C}} },
    { 'E', {{0x7F,0x49,0x49,0x49,0x41}} },
    { 'F', {{0x7F,0x09,0x09,0x09,0x01}} },
    { 'G', {{0x3E,0x41,0x49,0x49,0x7A}} },
    { 'H', {{0x7F,0x08,0x08,0x08,0x7F}} },
    { 'I', {{0x00,0x41,0x7F,0x41,0x00}} },
    { 'J', {{0x20,0x40,0x41,0x3F,0x01}} },
    { 'K', {{0x7F,0x08,0x14,0x22,0x41}} },
    { 'L', {{0x7F,0x40,0x40,0x40,0x40}} },
    { 'M', {{0x7F,0x02,0x0C,0x02,0x7F}} },
    { 'N', {{0x7F,0x04,0x08,0x10,0x7F}} },
    { 'O', {{0x3E,0x41,0x41,0x41,0x3E}} },
    { 'P', {{0x7F,0x09,0x09,0x09,0x06}} },
    { 'Q', {{0x3E,0x41,0x51,0x21,0x5E}} },
    { 'R', {{0x7F,0x09,0x19,0x29,0x46}} },
    { 'S', {{0x46,0x49,0x49,0x49,0x31}} },
    { 'T', {{0x01,0x01,0x7F,0x01,0x01}} },
    { 'U', {{0x3F,0x40,0x40,0x40,0x3F}} },
    { 'V', {{0x1F,0x20,0x40,0x20,0x1F}} },
    { 'W', {{0x7F,0x20,0x18,0x20,0x7F}} },
    { 'X', {{0x63,0x14,0x08,0x14,0x63}} },
    { 'Y', {{0x07,0x08,0x70,0x08,0x07}} },
    { 'Z', {{0x61,0x51,0x49,0x45,0x43}} },

    // Symbols
    { '[', {{0x00,0x7F,0x41,0x41,0x00}} },
    { '\\',{{0x02,0x04,0x08,0x10,0x20}} },
    { ']', {{0x00,0x41,0x41,0x7F,0x00}} },
    { '^', {{0x04,0x02,0x01,0x02,0x04}} },
    { '_', {{0x40,0x40,0x40,0x40,0x40}} },
    { '`', {{0x00,0x01,0x02,0x04,0x00}} },

    // Lowercase a–z
    { 'a', {{0x20,0x54,0x54,0x54,0x78}} },
    { 'b', {{0x7F,0x48,0x44,0x44,0x38}} },
    { 'c', {{0x38,0x44,0x44,0x44,0x20}} },
    { 'd', {{0x38,0x44,0x44,0x48,0x7F}} },
    { 'e', {{0x38,0x54,0x54,0x54,0x18}} },
    { 'f', {{0x08,0x7E,0x09,0x01,0x02}} },
    { 'g', {{0x0C,0x52,0x52,0x52,0x3E}} },
    { 'h', {{0x7F,0x08,0x04,0x04,0x78}} },
    { 'i', {{0x00,0x44,0x7D,0x40,0x00}} },
    { 'j', {{0x20,0x40,0x44,0x3D,0x00}} },
    { 'k', {{0x7F,0x10,0x28,0x44,0x00}} },
    { 'l', {{0x00,0x41,0x7F,0x40,0x00}} },
    { 'm', {{0x7C,0x04,0x18,0x04,0x78}} },
    { 'n', {{0x7C,0x08,0x04,0x04,0x78}} },
    { 'o', {{0x38,0x44,0x44,0x44,0x38}} },
    { 'p', {{0x7C,0x14,0x14,0x14,0x08}} },
    { 'q', {{0x08,0x14,0x14,0x18,0x7C}} },
    { 'r', {{0x7C,0x08,0x04,0x04,0x08}} },
    { 's', {{0x48,0x54,0x54,0x54,0x20}} },
    { 't', {{0x04,0x3F,0x44,0x40,0x20}} },
    { 'u', {{0x3C,0x40,0x40,0x20,0x7C}} },
    { 'v', {{0x1C,0x20,0x40,0x20,0x1C}} },
    { 'w', {{0x3C,0x40,0x30,0x40,0x3C}} },
    { 'x', {{0x44,0x28,0x10,0x28,0x44}} },
    { 'y', {{0x0C,0x50,0x50,0x50,0x3C}} },
    { 'z', {{0x44,0x64,0x54,0x4C,0x44}} },

    // Extra
    { '{', {{0x00,0x08,0x36,0x41,0x00}} },
    { '|', {{0x00,0x00,0x7F,0x00,0x00}} },
    { '}', {{0x00,0x41,0x36,0x08,0x00}} },
    { '~', {{0x08,0x04,0x08,0x10,0x08}} }
};

static void buildTextBitmap(const std::string& text, int scale,
                            std::vector<unsigned char>& out, int& w, int& h)
{
    const int gw = 5, gh = 7, spacing = 1;
    const int cw = (gw + spacing) * scale;
    const int ch = gh * scale;

    w = int(text.size()) * cw;
    h = ch;
    if (w <= 0 || h <= 0) { out.clear(); return; }

    out.assign(w * h, 0);

    int xoff = 0;
    for (char c : text) {
        auto it = kFont.find(c);
        const Glyph& g = (it != kFont.end()) ? it->second : kFont.at('?');

        for (int col = 0; col < gw; ++col) {
            unsigned char bits = g.col[col];
            for (int row = 0; row < gh; ++row) {
                bool on = (bits >> row) & 1; // row 0 = top
                if (!on) continue;
                int px0 = xoff + col * scale;
                int py0 = (gh - 1 - row) * scale; // flip to make row0 top
                for (int sy = 0; sy < scale; ++sy)
                    for (int sx = 0; sx < scale; ++sx) {
                        int px = px0 + sx, py = py0 + sy;
                        out[py * w + px] = 255;
                    }
            }
        }
        xoff += cw;
    }
}

// ---------- Per-window state ----------
struct CameraState {
    glm::vec3 pos{0.f, 16.f, 16.f}; // start above terrain
    float yaw   = -90.0f;
    float pitch =   0.0f;
    bool   firstMouse = true;
    double lastX = 0.0, lastY = 0.0;
    float  sensitivity = 0.1f;
};

struct AppState {
    CameraState cam;
    int fbw = 1280, fbh = 720;
    bool debugWireframe = false;

    // Option A: cached projections (recomputed on resize)
    float fov   = 60.f;
    float znear = 0.1f;
    float zfar  = 2000.f;
    glm::mat4 proj3D{1.0f};
    glm::mat4 projHUD{1.0f};
};

// ---------- Callbacks ----------
static void FramebufferSizeCallback(GLFWwindow* window, int w, int h) {
    auto* app = static_cast<AppState*>(glfwGetWindowUserPointer(window));
    if (!app) return;

    app->fbw = (w > 0 ? w : 1);
    app->fbh = (h > 0 ? h : 1);

    glViewport(0, 0, app->fbw, app->fbh);

    // Recompute cached projections (Option A)
    float aspect = float(app->fbw) / float(app->fbh);
    app->proj3D = glm::perspective(glm::radians(app->fov), aspect, app->znear, app->zfar);
    app->projHUD = glm::ortho(0.f, float(app->fbw), 0.f, float(app->fbh));
}

static void CursorPosCallback(GLFWwindow* window, double xpos, double ypos) {
    auto* app = static_cast<AppState*>(glfwGetWindowUserPointer(window));
    if (!app) return;
    auto& cam = app->cam;
    if (cam.firstMouse) { cam.firstMouse=false; cam.lastX=xpos; cam.lastY=ypos; return; }
    double dx = xpos - cam.lastX;
    double dy = cam.lastY - ypos; // invert Y
    cam.lastX = xpos; cam.lastY = ypos;
    cam.yaw   += float(dx) * cam.sensitivity;
    cam.pitch += float(dy) * cam.sensitivity;
    cam.pitch = std::clamp(cam.pitch, -89.f, 89.f);
}

// ---------- Main ----------
int main() {
    if (!glfwInit()) die("glfwInit failed");
    glfwWindowHint(GLFW_CONTEXT_VERSION_MAJOR, 3);
    glfwWindowHint(GLFW_CONTEXT_VERSION_MINOR, 3);
    glfwWindowHint(GLFW_OPENGL_PROFILE, GLFW_OPENGL_CORE_PROFILE);

    const int winW = 1280, winH = 720;
    GLFWwindow* win = glfwCreateWindow(winW, winH, "Wuffelland - Boot", nullptr, nullptr);
    if (!win) die("glfwCreateWindow failed");
    glfwMakeContextCurrent(win);
    if (!gladLoadGLLoader((GLADloadproc)glfwGetProcAddress)) die("GLAD init failed");
    glfwSwapInterval(1);

    AppState app{};
    app.fbw = winW; app.fbh = winH;

    // Initialize cached projections (Option A)
    {
        float aspect = float(app.fbw) / float(app.fbh);
        app.proj3D = glm::perspective(glm::radians(app.fov), aspect, app.znear, app.zfar);
        app.projHUD = glm::ortho(0.f, float(app.fbw), 0.f, float(app.fbh));
    }

    glfwSetWindowUserPointer(win, &app);
    glfwSetFramebufferSizeCallback(win, FramebufferSizeCallback);
    glfwSetCursorPosCallback(win,   CursorPosCallback);
    glfwSetInputMode(win, GLFW_CURSOR, GLFW_CURSOR_DISABLED);

    // 3D pipeline
    glEnable(GL_DEPTH_TEST);
    glEnable(GL_CULL_FACE);
    glCullFace(GL_BACK);

    GLuint prog3D = makeProgram(kVert3D, kFrag3D);
    glUseProgram(prog3D);

    #if 0
    // Cube geometry (CCW from outside)
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

    addFace(p100, p110, p111, p101, {+1,0,0}); // +X
    addFace(p000, p001, p011, p010, {-1,0,0}); // -X
    addFace(p010, p011, p111, p110, {0,+1,0}); // +Y
    addFace(p000, p100, p101, p001, {0,-1,0}); // -Y
    addFace(p001, p101, p111, p011, {0,0,+1}); // +Z
    addFace(p000, p010, p110, p100, {0,0,-1}); // -Z

    GLuint vao3D, vbo3D, ebo3D;
    glGenVertexArrays(1,&vao3D);
    glGenBuffers(1,&vbo3D);
    glGenBuffers(1,&ebo3D);
    glBindVertexArray(vao3D);
    glBindBuffer(GL_ARRAY_BUFFER, vbo3D);
    glBufferData(GL_ARRAY_BUFFER, verts.size()*sizeof(V), verts.data(), GL_STATIC_DRAW);
    glBindBuffer(GL_ELEMENT_ARRAY_BUFFER, ebo3D);
    glBufferData(GL_ELEMENT_ARRAY_BUFFER, idx.size()*sizeof(unsigned), idx.data(), GL_STATIC_DRAW);
    glVertexAttribPointer(0,3,GL_FLOAT,GL_FALSE,sizeof(V),(void*)offsetof(V,p)); glEnableVertexAttribArray(0);
    glVertexAttribPointer(1,3,GL_FLOAT,GL_FALSE,sizeof(V),(void*)offsetof(V,n)); glEnableVertexAttribArray(1);
    glVertexAttribPointer(2,2,GL_FLOAT,GL_FALSE,sizeof(V),(void*)offsetof(V,uv)); glEnableVertexAttribArray(2);
    #else

    // Checker texture
    const int TW=64, TH=64;
    std::vector<unsigned char> tex(TW*TH*3);
    for(int y=0;y<TH;++y) for(int x=0;x<TW;++x){
        int c = ((x/8)+(y/8)) & 1;
        unsigned char v = c ? 220 : 70;
        tex[(y*TW+x)*3+0]=v; tex[(y*TW+x)*3+1]=v; tex[(y*TW+x)*3+2]=v;
    }
    GLuint texId;
    glGenTextures(1,&texId);
    glBindTexture(GL_TEXTURE_2D, texId);
    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGB8, TW, TH, 0, GL_RGB, GL_UNSIGNED_BYTE, tex.data());
    glGenerateMipmap(GL_TEXTURE_2D);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR_MIPMAP_LINEAR);
    glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);

    GLint uMVP3D      = glGetUniformLocation(prog3D, "uMVP");
    GLint uModel3D    = glGetUniformLocation(prog3D, "uModel");
    GLint uLightDir3D = glGetUniformLocation(prog3D, "uLightDir");
    GLint uTex3D      = glGetUniformLocation(prog3D, "uTex");
    glUniform1i(uTex3D, 0);

    // ---------- WORLD: build 3x3 chunks ----------
    struct GpuChunk {
        Chunk chunk;
        GLuint vao=0, vbo=0, ebo=0;
        GLsizei indexCount=0;
        explicit GpuChunk(int cx,int cz) : chunk(cx,cz) {}
    };

    std::vector<GpuChunk> world;
    world.reserve(9);
    for (int dz=-1; dz<=1; ++dz){
        for (int dx=-1; dx<=1; ++dx){
            world.emplace_back(dx, dz);
        }
    }

    // Generate CPU meshes (flat terrain for now)
    for (auto& gc : world){
        gc.chunk.generateFlat();
        gc.chunk.buildMesh();
    }

    // Upload each chunk to GPU
    for (auto& gc : world){
        const auto& vv = gc.chunk.verts;
        const auto& ii = gc.chunk.indices;

        glGenVertexArrays(1, &gc.vao);
        glGenBuffers(1, &gc.vbo);
        glGenBuffers(1, &gc.ebo);
        glBindVertexArray(gc.vao);

        glBindBuffer(GL_ARRAY_BUFFER, gc.vbo);
        glBufferData(GL_ARRAY_BUFFER, vv.size()*sizeof(VoxelVertex), vv.data(), GL_STATIC_DRAW);

        glBindBuffer(GL_ELEMENT_ARRAY_BUFFER, gc.ebo);
        glBufferData(GL_ELEMENT_ARRAY_BUFFER, ii.size()*sizeof(unsigned), ii.data(), GL_STATIC_DRAW);

        // vertex format matches kVert3D (pos, normal, uv)
        glVertexAttribPointer(0,3,GL_FLOAT,GL_FALSE,sizeof(VoxelVertex),(void*)offsetof(VoxelVertex,p)); glEnableVertexAttribArray(0);
        glVertexAttribPointer(1,3,GL_FLOAT,GL_FALSE,sizeof(VoxelVertex),(void*)offsetof(VoxelVertex,n)); glEnableVertexAttribArray(1);
        glVertexAttribPointer(2,2,GL_FLOAT,GL_FALSE,sizeof(VoxelVertex),(void*)offsetof(VoxelVertex,uv)); glEnableVertexAttribArray(2);

        gc.indexCount = (GLsizei)ii.size();
    }
    #endif

    // ---------- HUD TEXT setup ----------
    GLuint progHUD = makeProgram(kVertHUD, kFragHUD);

    GLuint vaoHUD, vboHUD;
    glGenVertexArrays(1, &vaoHUD);
    glGenBuffers(1, &vboHUD);
    glBindVertexArray(vaoHUD);
    glBindBuffer(GL_ARRAY_BUFFER, vboHUD);
    glBufferData(GL_ARRAY_BUFFER, sizeof(float)*16, nullptr, GL_DYNAMIC_DRAW); // 4 verts * (x,y,u,v)
    glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, sizeof(float)*4, (void*)0);             glEnableVertexAttribArray(0);
    glVertexAttribPointer(1, 2, GL_FLOAT, GL_FALSE, sizeof(float)*4, (void*)(sizeof(float)*2)); glEnableVertexAttribArray(1);

    GLuint fontTex = 0;
    glGenTextures(1, &fontTex);

    glEnable(GL_BLEND);
    glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);

    auto uploadFontTexture = [&](const std::vector<unsigned char>& alpha, int w, int h) {
        glBindTexture(GL_TEXTURE_2D, fontTex);
        glTexImage2D(GL_TEXTURE_2D, 0, GL_R8, w, h, 0, GL_RED, GL_UNSIGNED_BYTE, alpha.data());
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
        glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
    };

    // Timing / FPS
    auto lastTime = std::chrono::high_resolution_clock::now();
    double accTitle = 0.0; int frames = 0;

    while(!glfwWindowShouldClose(win)){
        auto now = std::chrono::high_resolution_clock::now();
        float dt = std::chrono::duration<float>(now - lastTime).count();
        lastTime = now;

        // Camera movement
        glm::vec3 front{
            std::cos(glm::radians(app.cam.yaw)) * std::cos(glm::radians(app.cam.pitch)),
            std::sin(glm::radians(app.cam.pitch)),
            std::sin(glm::radians(app.cam.yaw)) * std::cos(glm::radians(app.cam.pitch))
        };
        glm::vec3 fwd   = glm::normalize(front);
        glm::vec3 right = glm::normalize(glm::cross(fwd, {0,1,0}));
        const float move = 8.0f * dt;
        if (glfwGetKey(win, GLFW_KEY_W) == GLFW_PRESS) app.cam.pos += fwd   * move;
        if (glfwGetKey(win, GLFW_KEY_S) == GLFW_PRESS) app.cam.pos -= fwd   * move;
        if (glfwGetKey(win, GLFW_KEY_A) == GLFW_PRESS) app.cam.pos -= right * move;
        if (glfwGetKey(win, GLFW_KEY_D) == GLFW_PRESS) app.cam.pos += right * move;
        if (glfwGetKey(win, GLFW_KEY_SPACE) == GLFW_PRESS)      app.cam.pos.y += move;
        if (glfwGetKey(win, GLFW_KEY_LEFT_SHIFT) == GLFW_PRESS) app.cam.pos.y -= move;
        if (glfwGetKey(win, GLFW_KEY_ESCAPE) == GLFW_PRESS) glfwSetWindowShouldClose(win, true);

        // Toggle debug mode (wireframe) with F12
        static bool f12Pressed = false;
        if (glfwGetKey(win, GLFW_KEY_F12) == GLFW_PRESS) {
            if (!f12Pressed) {
                app.debugWireframe = !app.debugWireframe;
                f12Pressed = true;
            }
        } else {
            f12Pressed = false;
        }

        // F12 decided if debug mode is enabled (the low-poly wireframe mode):
        if (app.debugWireframe) {
            glPolygonMode(GL_FRONT_AND_BACK, GL_LINE);
            glDisable(GL_CULL_FACE);
        } else {
            glPolygonMode(GL_FRONT_AND_BACK, GL_FILL);
            glEnable(GL_CULL_FACE);
            glCullFace(GL_BACK);
        }

        // ----- 3D pass -----
        glViewport(0,0,app.fbw,app.fbh);
        glClearColor(0.15f, 0.18f, 0.23f, 1.0f);
        glClear(GL_COLOR_BUFFER_BIT|GL_DEPTH_BUFFER_BIT);

        glm::mat4 view  = glm::lookAt(app.cam.pos, app.cam.pos + fwd, {0,1,0});

        #if 0
        float t = (float)glfwGetTime();
        glm::mat4 model(1.0f);
        model = glm::rotate(model, t*0.7f, {0,1,0});
        model = glm::rotate(model, t*0.31f, {1,0,0});
        glm::mat4 mvp = app.proj3D * view * model;

        glUseProgram(prog3D);
        glUniformMatrix4fv(uMVP3D,   1, GL_FALSE, glm::value_ptr(mvp));
        glUniformMatrix4fv(uModel3D, 1, GL_FALSE, glm::value_ptr(model));
        glUniform3f(uLightDir3D, -0.6f, -1.0f, -0.2f);
        glActiveTexture(GL_TEXTURE0);
        glBindTexture(GL_TEXTURE_2D, texId);
        glBindVertexArray(vao3D);
        glDrawElements(GL_TRIANGLES, (GLsizei)idx.size(), GL_UNSIGNED_INT, 0);
        #else
        glUseProgram(prog3D);
        glUniform3f(uLightDir3D, -0.6f, -1.0f, -0.2f);
        glActiveTexture(GL_TEXTURE0);
        glBindTexture(GL_TEXTURE_2D, texId);

        // Draw all chunks (model = identity; chunk coords baked into vertices)
        for (auto& gc : world){
            glm::mat4 model(1.0f);
            glm::mat4 mvp = app.proj3D * view * model;
            glUniformMatrix4fv(uMVP3D,   1, GL_FALSE, glm::value_ptr(mvp));
            glUniformMatrix4fv(uModel3D, 1, GL_FALSE, glm::value_ptr(model));
        
            glBindVertexArray(gc.vao);
            glDrawElements(GL_TRIANGLES, gc.indexCount, GL_UNSIGNED_INT, 0);
        }
        #endif

        // ----- HUD pass (orthographic) -----
        const std::string titleStr = "Wuffelland";
        int desiredPx = std::max(1, (int)std::floor(app.fbh * 0.03f)); // ≤3% height
        int scale = std::max(1, desiredPx / 7); // 7 px baseline
        std::vector<unsigned char> alpha;
        int texW=0, texH=0;
        buildTextBitmap(titleStr, scale, alpha, texW, texH);
        uploadFontTexture(alpha, texW, texH);

        float x = (app.fbw - texW) * 0.5f;   // center horizontally
        float y = app.fbh - texH - 10.0f;    // margin from top

        glm::mat4 projHUD = app.projHUD;

       // UVs: v bottom=0, top=1 (matches buildTextBitmap write)
       float quad[16] = {
            x,       y,        0.f, 0.f, // BL
            x+texW,  y,        1.f, 0.f, // BR
            x,       y+texH,   0.f, 1.f, // TL
            x+texW,  y+texH,   1.f, 1.f  // TR
        };

        GLuint prog = progHUD;
        glUseProgram(prog);
        GLint uPHUD   = glGetUniformLocation(prog, "uProj");
        GLint uColor  = glGetUniformLocation(prog, "uColor");
        GLint uFont   = glGetUniformLocation(prog, "uFont");
        glUniformMatrix4fv(uPHUD, 1, GL_FALSE, glm::value_ptr(projHUD));
        glUniform3f(uColor, 1.0f, 1.0f, 1.0f);
        glUniform1i(uFont, 0);

        glActiveTexture(GL_TEXTURE0);
        glBindTexture(GL_TEXTURE_2D, fontTex);

        glBindVertexArray(vaoHUD);
        glBindBuffer(GL_ARRAY_BUFFER, vboHUD);
        glBufferSubData(GL_ARRAY_BUFFER, 0, sizeof(quad), quad);
        glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);

        // ----- swap/poll -----
        glfwSwapBuffers(win);
        glfwPollEvents();

        // fps in title
        frames++; accTitle += dt;
        if (accTitle >= 1.0) {
            char titleBuf[128];
            std::snprintf(titleBuf, sizeof(titleBuf), "Wuffelland - Boot  |  FPS: %d  |  dt: %.2f ms",
                          frames, 1000.0/frames);
            glfwSetWindowTitle(win, titleBuf);
            frames = 0; accTitle = 0.0;
        }
    }

    glfwTerminate();
    return 0;
}
