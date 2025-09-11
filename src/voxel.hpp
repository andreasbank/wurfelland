#pragma once
#include <vector>
#include <cstdint>
#include <glm/glm.hpp>

constexpr int CHUNK_W = 16;
constexpr int CHUNK_H = 64;
constexpr int CHUNK_D = 16;

enum class Block : uint8_t { Air=0, Grass, Dirt, Stone };

inline bool isSolid(Block b){ return b != Block::Air; }

// Vertex layout matches your 3D shader: pos, normal, uv
struct VoxelVertex {
    glm::vec3 p;
    glm::vec3 n;
    glm::vec2 uv;
};

// Simple chunk at (cx, cz) in chunk-space; world coords = chunkOrigin + local
struct Chunk {
    int cx=0, cz=0; // chunk coordinates in X-Z
    std::vector<Block> blocks; // size CHUNK_W*CHUNK_H*CHUNK_D

    // Mesh buffers
    std::vector<VoxelVertex> verts;
    std::vector<unsigned>   indices;

    Chunk(int cx_, int cz_) : cx(cx_), cz(cz_), blocks(CHUNK_W*CHUNK_H*CHUNK_D, Block::Air) {}

    static inline int idx(int x,int y,int z){
        return (y*CHUNK_D + z)*CHUNK_W + x;
    }

    inline Block get(int x,int y,int z) const {
        if (x<0||x>=CHUNK_W||y<0||y>=CHUNK_H||z<0||z>=CHUNK_D) return Block::Air;
        return blocks[idx(x,y,z)];
    }
    inline void set(int x,int y,int z, Block b){
        if (x<0||x>=CHUNK_W||y<0||y>=CHUNK_H||z<0||z>=CHUNK_D) return;
        blocks[idx(x,y,z)] = b;
    }

    // Fill with a simple flat terrain:
    // height = 32: grass top, 3 dirt below, then stone
    void generateFlat(){
        int hBase = 32;
        for(int z=0; z<CHUNK_D; ++z){
            for(int x=0; x<CHUNK_W; ++x){
                for(int y=0; y<CHUNK_H; ++y){
                    int wy = y;
                    if (wy > hBase) { set(x,y,z, Block::Air); continue; }
                    if (wy == hBase) set(x,y,z, Block::Grass);
                    else if (wy >= hBase-3) set(x,y,z, Block::Dirt);
                    else set(x,y,z, Block::Stone);
                }
            }
        }
    }

    // Build a mesh of only exposed faces (naive mesher; greedy comes later)
    void buildMesh(){
        verts.clear(); indices.clear();
        verts.reserve( CHUNK_W*CHUNK_H*CHUNK_D * 24 ); // worst-case rough
        indices.reserve( CHUNK_W*CHUNK_H*CHUNK_D * 36 );

        // World origin of this chunk in blocks
        const int originX = cx * CHUNK_W;
        const int originZ = cz * CHUNK_D;

        auto pushFace = [&](glm::vec3 a, glm::vec3 b, glm::vec3 c, glm::vec3 d,
                            const glm::vec3& n){
            unsigned base = (unsigned)verts.size();
            verts.push_back({a,n,{0,0}});
            verts.push_back({b,n,{1,0}});
            verts.push_back({c,n,{1,1}});
            verts.push_back({d,n,{0,1}});
            indices.push_back(base+0); indices.push_back(base+1); indices.push_back(base+2);
            indices.push_back(base+0); indices.push_back(base+2); indices.push_back(base+3);
        };

        for (int y=0; y<CHUNK_H; ++y){
            for (int z=0; z<CHUNK_D; ++z){
                for (int x=0; x<CHUNK_W; ++x){
                    Block b = get(x,y,z);
                    if (!isSolid(b)) continue;

                    // world-space block min corner
                    float wx = float(originX + x);
                    float wy = float(y);
                    float wz = float(originZ + z);

                    // Adjacent blocks (air => face visible)
                    Block nxm = (x>0)            ? get(x-1,y,z) : Block::Air;
                    Block nxp = (x<CHUNK_W-1)    ? get(x+1,y,z) : Block::Air;
                    Block nym = (y>0)            ? get(x,y-1,z) : Block::Air;
                    Block nyp = (y<CHUNK_H-1)    ? get(x,y+1,z) : Block::Air;
                    Block nzm = (z>0)            ? get(x,y,z-1) : Block::Air;
                    Block nzp = (z<CHUNK_D-1)    ? get(x,y,z+1) : Block::Air;

                    // +X face
                    if (!isSolid(nxp)){
                        glm::vec3 a{wx+1, wy+0, wz+0};
                        glm::vec3 b{wx+1, wy+0, wz+1};
                        glm::vec3 c{wx+1, wy+1, wz+1};
                        glm::vec3 d{wx+1, wy+1, wz+0};
                        pushFace(a,b,c,d, {+1,0,0});
                    }
                    // -X face
                    if (!isSolid(nxm)){
                        glm::vec3 a{wx+0, wy+0, wz+1};
                        glm::vec3 b{wx+0, wy+0, wz+0};
                        glm::vec3 c{wx+0, wy+1, wz+0};
                        glm::vec3 d{wx+0, wy+1, wz+1};
                        pushFace(a,b,c,d, {-1,0,0});
                    }
                    // +Y face (top)
                    if (!isSolid(nyp)){
                        glm::vec3 a{wx+0, wy+1, wz+0};
                        glm::vec3 b{wx+1, wy+1, wz+0};
                        glm::vec3 c{wx+1, wy+1, wz+1};
                        glm::vec3 d{wx+0, wy+1, wz+1};
                        pushFace(a,b,c,d, {0,+1,0});
                    }
                    // -Y face (bottom)
                    if (!isSolid(nym)){
                        glm::vec3 a{wx+1, wy+0, wz+0};
                        glm::vec3 b{wx+0, wy+0, wz+0};
                        glm::vec3 c{wx+0, wy+0, wz+1};
                        glm::vec3 d{wx+1, wy+0, wz+1};
                        pushFace(a,b,c,d, {0,-1,0});
                    }
                    // +Z face (front)
                    if (!isSolid(nzp)){
                        glm::vec3 a{wx+0, wy+0, wz+1};
                        glm::vec3 b{wx+1, wy+0, wz+1};
                        glm::vec3 c{wx+1, wy+1, wz+1};
                        glm::vec3 d{wx+0, wy+1, wz+1};
                        pushFace(a,b,c,d, {0,0,+1});
                    }
                    // -Z face (back)
                    if (!isSolid(nzm)){
                        glm::vec3 a{wx+1, wy+0, wz+0};
                        glm::vec3 b{wx+0, wy+0, wz+0};
                        glm::vec3 c{wx+0, wy+1, wz+0};
                        glm::vec3 d{wx+1, wy+1, wz+0};
                        pushFace(a,b,c,d, {0,0,-1});
                    }
                }
            }
        }
    }
};
