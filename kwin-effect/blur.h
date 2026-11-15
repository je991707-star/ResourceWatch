/*
    SPDX-FileCopyrightText: 2010 Fredrik Höglund <fredrik@kde.org>
    SPDX-FileCopyrightText: 2018 Alex Nemeth <alex.nemeth329@gmail.com>

    SPDX-License-Identifier: GPL-2.0-or-later

    Fork of KWin's blur effect that only blurs ResourceWatch windows, with
    parameters read from ~/.config/resourcewatch/kwin-blur.ini.
*/

#pragma once

#include "effect/effect.h"
#include "opengl/glutils.h"
#include "scene/item.h"

#include <QFileSystemWatcher>
#include <QList>

#include <unordered_map>

namespace KWin
{

class BackgroundEffectItem;

struct BlurRenderData
{
    /// Temporary render targets needed for the Dual Kawase algorithm, the first texture
    /// contains not blurred background behind the window, it's cached.
    std::vector<std::unique_ptr<GLTexture>> textures;
    std::vector<std::unique_ptr<GLFramebuffer>> framebuffers;
};

struct BlurEffectData
{
    /// Per render view, as they can have different color spaces.
    std::unordered_map<RenderView *, BlurRenderData> render;

    std::unique_ptr<BackgroundEffectItem> blurItem;
};

class RwBlurEffect : public KWin::Effect
{
    Q_OBJECT

public:
    RwBlurEffect();

    static bool supported();

    void prePaintScreen(ScreenPrePaintData &data) override;
    bool drawWindow(const RenderTarget &renderTarget, const RenderViewport &viewport, EffectWindow *w, int mask, const Region &deviceRegion, WindowPaintData &data) override;

    bool isActive() const override;

    int requestedEffectChainPosition() const override
    {
        return 20;
    }

    bool blocksDirectScanout() const override;

public Q_SLOTS:
    void slotWindowAdded(KWin::EffectWindow *w);
    void slotWindowDeleted(KWin::EffectWindow *w);
    void slotViewRemoved(KWin::RenderView *view);

private:
    void initBlurStrengthValues();
    void loadConfig();
    void updateWindow(EffectWindow *w);
    void blur(const RenderTarget &renderTarget, const RenderViewport &viewport, EffectWindow *w, int mask, const Region &deviceRegion, WindowPaintData &data);
    GLTexture *ensureNoiseTexture();

private:
    struct
    {
        std::unique_ptr<GLShader> shader;
        int mvpMatrixLocation;
        int colorMatrixLocation;
        int offsetLocation;
        int halfpixelLocation;
        int boxLocation;
        int cornerRadiusLocation;
        int opacityLocation;
    } m_roundedOnscreenPass;

    struct
    {
        std::unique_ptr<GLShader> shader;
        int mvpMatrixLocation;
        int offsetLocation;
        int halfpixelLocation;
    } m_downsamplePass;

    struct
    {
        std::unique_ptr<GLShader> shader;
        int mvpMatrixLocation;
        int offsetLocation;
        int halfpixelLocation;
    } m_upsamplePass;

    struct
    {
        std::unique_ptr<GLShader> shader;
        int mvpMatrixLocation;
        int noiseTextureSizeLocation;

        std::unique_ptr<GLTexture> noiseTexture;
        qreal noiseTextureScale = 1.0;
        int noiseTextureStength = 0;
    } m_noisePass;

    bool m_valid = false;
    RenderView *m_currentView = nullptr;

    // From kwin-blur.ini; nothing is blurred until the app writes enabled=true.
    bool m_enabled = false;
    QMatrix4x4 m_colorMatrix;
    size_t m_iterationCount = 1; // number of times the texture will be downsized to half size
    float m_offset = 1;
    int m_expandSize = 10;
    int m_noiseStrength = 0;
    float m_radius = 0; // window corner radius, logical px

    QString m_configDir;
    QFileSystemWatcher m_watcher;

    struct OffsetStruct
    {
        float minOffset;
        float maxOffset;
        int expandSize;
    };

    QList<OffsetStruct> blurOffsets;

    struct BlurValuesStruct
    {
        int iteration;
        float offset;
    };

    QList<BlurValuesStruct> blurStrengthValues;

    std::unordered_map<EffectWindow *, BlurEffectData> m_windows;
};

} // namespace KWin
