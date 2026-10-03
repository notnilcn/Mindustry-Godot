// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla weather registry.
//!
//! Ported from `core/src/mindustry/content/Weathers.java` (6 weathers, exact
//! `load()` order), `core/src/mindustry/type/Weather.java` and
//! `type/weather/{ParticleWeather,RainWeather,MagneticStorm,SolarFlare}.java`
//! (metadata; rendering/update in plans 05/16).

use super::super::bundle::BundleView;
use super::super::color::Rgba;
use super::super::ctype::{Content, Mappable, ModContentInfo, UnlockFields, Unlockable};
use super::super::id::{LiquidId, StatusId, WeatherId};
use super::super::load::ContentRegistry;
use super::super::settings_store::UnlockStore;
use super::super::{ContentError, ContentType};

/// `Time.toMinutes` (arc): one minute in ticks.
pub const TIME_TO_MINUTES: f32 = 60.0 * 60.0;

/// Environment attribute tag (`Attribute.light`/`water`/`spores`/`heat`/`steam`).
///
/// Append-only: `Heat..Light` keep the indices this port shipped with; `Steam`
/// is appended (plan 09 R3 / plan 02) so existing attribute arrays and checksums
/// are unchanged. Upstream's `oil`/`sand` are still unmodelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Attribute {
    /// `Attribute.heat`.
    Heat,
    /// `Attribute.spores`.
    Spores,
    /// `Attribute.water`.
    Water,
    /// `Attribute.light`.
    Light,
    /// `Attribute.steam` (Erekir vents; `turbine-condenser`).
    Steam,
}

impl Attribute {
    /// Java enum identifier.
    pub const fn name(self) -> &'static str {
        match self {
            Attribute::Heat => "heat",
            Attribute::Spores => "spores",
            Attribute::Water => "water",
            Attribute::Light => "light",
            Attribute::Steam => "steam",
        }
    }
}

/// Weather class tag (`Prov<WeatherState>` kind; plan 05 dispatches behavior).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeatherKind {
    /// Plain `Weather`.
    Plain,
    /// `ParticleWeather`.
    Particle,
    /// `RainWeather`.
    Rain,
    /// `MagneticStorm` (stub upstream).
    MagneticStorm,
    /// `SolarFlare` (stub upstream).
    SolarFlare,
}

impl WeatherKind {
    /// Java class name.
    pub const fn name(self) -> &'static str {
        match self {
            WeatherKind::Plain => "Weather",
            WeatherKind::Particle => "ParticleWeather",
            WeatherKind::Rain => "RainWeather",
            WeatherKind::MagneticStorm => "MagneticStorm",
            WeatherKind::SolarFlare => "SolarFlare",
        }
    }
}

/// `ParticleWeather` fields.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleWeatherFields {
    /// Particle atlas region name.
    pub particle_region: String,
    /// Weather tint color.
    pub color: Rgba,
    /// Noise tint color.
    pub noise_color: Rgba,
    /// Vertical particle speed.
    pub yspeed: f32,
    /// Horizontal particle speed.
    pub xspeed: f32,
    /// Particle padding.
    pub padding: f32,
    /// Minimum particle size.
    pub size_min: f32,
    /// Maximum particle size.
    pub size_max: f32,
    /// Particle density.
    pub density: f32,
    /// Minimum particle alpha.
    pub min_alpha: f32,
    /// Maximum particle alpha.
    pub max_alpha: f32,
    /// Wind force.
    pub force: f32,
    /// Noise texture scale.
    pub noise_scale: f32,
    /// Base particle speed.
    pub base_speed: f32,
    /// Sine scale range.
    pub sin_scl_min: f32,
    /// Sine scale range.
    pub sin_scl_max: f32,
    /// Sine magnitude range.
    pub sin_mag_min: f32,
    /// Sine magnitude range.
    pub sin_mag_max: f32,
    /// Whether to draw the noise overlay.
    pub draw_noise: bool,
    /// Whether to draw particles.
    pub draw_particles: bool,
    /// Whether particles use the wind vector.
    pub use_wind_vector: bool,
    /// Whether particle rotation is random.
    pub random_particle_rotation: bool,
    /// Number of noise layers.
    pub noise_layers: i32,
    /// Noise layer speed multiplier.
    pub noise_layer_speed_m: f32,
    /// Noise layer alpha multiplier.
    pub noise_layer_alpha_m: f32,
    /// Noise layer scale multiplier.
    pub noise_layer_scl_m: f32,
    /// Noise layer color multiplier.
    pub noise_layer_color_m: f32,
    /// Noise texture path.
    pub noise_path: String,
}

impl Default for ParticleWeatherFields {
    fn default() -> Self {
        Self {
            particle_region: String::from("circle-shadow"),
            color: Rgba::WHITE,
            noise_color: Rgba::WHITE,
            yspeed: -2.0,
            xspeed: 0.25,
            padding: 16.0,
            size_min: 2.4,
            size_max: 12.0,
            density: 1200.0,
            min_alpha: 1.0,
            max_alpha: 1.0,
            force: 0.0,
            noise_scale: 2000.0,
            base_speed: 6.1,
            sin_scl_min: 30.0,
            sin_scl_max: 80.0,
            sin_mag_min: 1.0,
            sin_mag_max: 7.0,
            draw_noise: false,
            draw_particles: true,
            use_wind_vector: false,
            random_particle_rotation: false,
            noise_layers: 1,
            noise_layer_speed_m: 1.1,
            noise_layer_alpha_m: 0.8,
            noise_layer_scl_m: 0.99,
            noise_layer_color_m: 1.0,
            noise_path: String::from("noiseAlpha"),
        }
    }
}

/// `RainWeather` fields.
#[derive(Debug, Clone, PartialEq)]
pub struct RainWeatherFields {
    /// Vertical rain speed.
    pub yspeed: f32,
    /// Horizontal rain speed.
    pub xspeed: f32,
    /// Padding.
    pub padding: f32,
    /// Rain density.
    pub density: f32,
    /// Stroke width.
    pub stroke: f32,
    /// Minimum splash size.
    pub size_min: f32,
    /// Maximum splash size.
    pub size_max: f32,
    /// Splash animation time scale.
    pub splash_time_scale: f32,
    /// Liquid deposited by rain.
    pub liquid: LiquidId,
    /// Rain color.
    pub color: Rgba,
    /// Splash region expectations (`splash-0..11`).
    pub splash_regions: u8,
}

impl Default for RainWeatherFields {
    fn default() -> Self {
        Self {
            yspeed: 5.0,
            xspeed: 1.5,
            padding: 16.0,
            density: 1200.0,
            stroke: 0.75,
            size_min: 8.0,
            size_max: 40.0,
            splash_time_scale: 22.0,
            liquid: LiquidId::WATER,
            color: Rgba::from_rgba8888(0x7a95eaff),
            splash_regions: 12,
        }
    }
}

/// Weather content record (`mindustry.type.Weather`).
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherDef {
    /// Dense id in the weather content space.
    pub id: WeatherId,
    /// Content name (parity ABI; `snow` loads as `snowing`).
    pub name: String,
    /// Mod/provenance info.
    pub minfo: ModContentInfo,
    /// Whether removed by a data patch.
    pub removed: bool,
    /// Unlock/database fields.
    pub unlock: UnlockFields,
    /// Class-kind tag.
    pub kind: WeatherKind,
    /// Weather duration in ticks (`10 * Time.toMinutes` default).
    pub duration: f32,
    /// Opacity multiplier.
    pub opacity_multiplier: f32,
    /// Environment attributes set by this weather.
    pub attrs: Vec<(Attribute, f32)>,
    /// Sound name (`Sounds` key).
    pub sound: Option<String>,
    /// Volume.
    pub sound_vol: f32,
    /// Minimum volume.
    pub sound_vol_min: f32,
    /// Volume oscillation magnitude.
    pub sound_vol_osc_mag: f32,
    /// Volume oscillation scale.
    pub sound_vol_osc_scl: f32,
    /// Hidden from most UI.
    pub hidden: bool,
    /// Status applied by the weather.
    pub status: StatusId,
    /// Applied status duration.
    pub status_duration: f32,
    /// Whether the status applies to air units.
    pub status_air: bool,
    /// Whether the status applies to ground units.
    pub status_ground: bool,
    /// Particle-weather fields, when applicable.
    pub particle: Option<ParticleWeatherFields>,
    /// Rain-weather fields, when applicable.
    pub rain: Option<RainWeatherFields>,
}

impl WeatherDef {
    /// Creates a weather with upstream defaults.
    pub fn new(
        name: &str,
        kind: WeatherKind,
        bundle: &dyn BundleView,
        store: &dyn UnlockStore,
    ) -> Self {
        let mut weather = Self {
            id: WeatherId::new(0),
            name: name.to_owned(),
            minfo: ModContentInfo::default(),
            removed: false,
            unlock: UnlockFields::new(ContentType::Weather, name, bundle, store),
            kind,
            duration: 10.0 * TIME_TO_MINUTES,
            opacity_multiplier: 1.0,
            attrs: Vec::new(),
            sound: None,
            sound_vol: 0.1,
            sound_vol_min: 0.0,
            sound_vol_osc_mag: 0.0,
            sound_vol_osc_scl: 20.0,
            hidden: false,
            status: StatusId::NONE,
            status_duration: 60.0 * 2.0,
            status_air: true,
            status_ground: true,
            particle: None,
            rain: None,
        };
        match kind {
            WeatherKind::Particle => weather.particle = Some(ParticleWeatherFields::default()),
            WeatherKind::Rain => weather.rain = Some(RainWeatherFields::default()),
            _ => {}
        }
        weather
    }

    /// Sets an attribute value (`Attributes.set` replace semantics).
    pub fn set_attr(&mut self, attribute: Attribute, value: f32) {
        if let Some(entry) = self
            .attrs
            .iter_mut()
            .find(|(existing, _)| *existing == attribute)
        {
            entry.1 = value;
        } else {
            self.attrs.push((attribute, value));
        }
    }

    /// Attribute value, `0.0` when unset.
    pub fn attr(&self, attribute: Attribute) -> f32 {
        self.attrs
            .iter()
            .find(|(existing, _)| *existing == attribute)
            .map(|(_, value)| *value)
            .unwrap_or(0.0)
    }

    /// Mutable particle fields.
    pub fn particle_mut(&mut self) -> &mut ParticleWeatherFields {
        self.particle
            .get_or_insert_with(ParticleWeatherFields::default)
    }

    /// Mutable rain fields.
    pub fn rain_mut(&mut self) -> &mut RainWeatherFields {
        self.rain.get_or_insert_with(RainWeatherFields::default)
    }
}

impl Content for WeatherDef {
    const TYPE: ContentType = ContentType::Weather;

    fn content_id(&self) -> u16 {
        self.id.raw()
    }

    fn set_content_id(&mut self, id: u16) {
        self.id = WeatherId::new(id);
    }

    fn minfo(&self) -> &ModContentInfo {
        &self.minfo
    }

    fn minfo_mut(&mut self) -> &mut ModContentInfo {
        &mut self.minfo
    }

    fn removed(&self) -> bool {
        self.removed
    }

    fn set_removed(&mut self, removed: bool) {
        self.removed = removed;
    }

    fn kind_name(&self) -> &'static str {
        self.kind.name()
    }

    fn content_name(&self) -> Option<&str> {
        Some(&self.name)
    }

    fn unlock_fields(&self) -> Option<&UnlockFields> {
        Some(&self.unlock)
    }

    fn post_init(&mut self) -> Result<(), ContentError> {
        self.unlock.post_init();
        Ok(())
    }
}

impl Mappable for WeatherDef {
    fn name(&self) -> &str {
        &self.name
    }
}

impl Unlockable for WeatherDef {
    fn unlock(&self) -> &UnlockFields {
        &self.unlock
    }

    fn unlock_mut(&mut self) -> &mut UnlockFields {
        &mut self.unlock
    }
}

/// Loads all 6 vanilla weathers in `Weathers.load()` order.
pub fn load(
    registry: &mut ContentRegistry,
    bundle: &dyn BundleView,
    store: &dyn UnlockStore,
) -> Result<(), ContentError> {
    let wet = registry.status_id("wet");
    let spore_slowed = registry.status_id("spore-slowed");

    // snow loads first, but its content name is `snowing`.
    registry.add_weather({
        let mut weather = WeatherDef::new("snowing", WeatherKind::Particle, bundle, store);
        weather.sound = Some(String::from("windHowl"));
        weather.sound_vol = 0.0;
        weather.sound_vol_osc_mag = 1.5;
        weather.sound_vol_osc_scl = 1100.0;
        weather.sound_vol_min = 0.02;
        weather.set_attr(Attribute::Light, -0.15);
        {
            let particle = weather.particle_mut();
            particle.particle_region = String::from("particle");
            particle.size_max = 13.0;
            particle.size_min = 2.6;
            particle.density = 1200.0;
        }
        weather
    })?;

    registry.add_weather({
        let mut weather = WeatherDef::new("rain", WeatherKind::Rain, bundle, store);
        weather.set_attr(Attribute::Light, -0.2);
        weather.set_attr(Attribute::Water, 0.2);
        weather.status = wet.unwrap_or(StatusId::NONE);
        weather.sound = Some(String::from("rain"));
        weather.sound_vol = 0.25;
        weather
    })?;

    registry.add_weather({
        let mut weather = WeatherDef::new("sandstorm", WeatherKind::Particle, bundle, store);
        weather.duration = 7.0 * TIME_TO_MINUTES;
        weather.opacity_multiplier = 0.35;
        weather.sound = Some(String::from("wind"));
        weather.sound_vol = 0.8;
        weather.set_attr(Attribute::Light, -0.1);
        weather.set_attr(Attribute::Water, -0.1);
        {
            let sand = Rgba::from_hex("f7cba4").unwrap_or(Rgba::WHITE);
            let particle = weather.particle_mut();
            particle.color = sand;
            particle.noise_color = sand;
            particle.particle_region = String::from("particle");
            particle.draw_noise = true;
            particle.use_wind_vector = true;
            particle.size_max = 140.0;
            particle.size_min = 70.0;
            particle.min_alpha = 0.0;
            particle.max_alpha = 0.2;
            particle.density = 1500.0;
            particle.base_speed = 5.4;
            particle.force = 0.1;
        }
        weather
    })?;

    registry.add_weather({
        let mut weather = WeatherDef::new("sporestorm", WeatherKind::Particle, bundle, store);
        weather.duration = 7.0 * TIME_TO_MINUTES;
        weather.opacity_multiplier = 0.5;
        weather.sound = Some(String::from("wind"));
        weather.sound_vol = 0.7;
        weather.status = spore_slowed.unwrap_or(StatusId::NONE);
        weather.status_ground = false;
        weather.set_attr(Attribute::Spores, 1.0);
        weather.set_attr(Attribute::Light, -0.15);
        {
            let spore = Rgba::from_hex("7457ce").unwrap_or(Rgba::WHITE);
            let particle = weather.particle_mut();
            particle.color = spore;
            particle.noise_color = spore;
            particle.particle_region = String::from("circle-small");
            particle.draw_noise = true;
            particle.use_wind_vector = true;
            particle.size_max = 5.0;
            particle.size_min = 2.5;
            particle.min_alpha = 0.1;
            particle.max_alpha = 0.8;
            particle.density = 2000.0;
            particle.base_speed = 4.3;
            particle.force = 0.1;
        }
        weather
    })?;

    registry.add_weather({
        let mut weather = WeatherDef::new("fog", WeatherKind::Particle, bundle, store);
        weather.duration = 15.0 * TIME_TO_MINUTES;
        weather.opacity_multiplier = 0.47;
        weather.set_attr(Attribute::Light, -0.3);
        weather.set_attr(Attribute::Water, 0.05);
        {
            let gray = Rgba::new(0.4, 0.4, 0.4, 1.0);
            let particle = weather.particle_mut();
            particle.color = gray;
            particle.noise_color = gray;
            particle.noise_layers = 3;
            particle.noise_layer_scl_m = 0.6;
            particle.noise_layer_alpha_m = 0.7;
            particle.noise_layer_speed_m = 2.0;
            particle.base_speed = 0.05;
            particle.noise_scale = 1100.0;
            particle.noise_path = String::from("fog");
            particle.draw_particles = false;
            particle.draw_noise = true;
            particle.use_wind_vector = false;
            particle.xspeed = 1.0;
            particle.yspeed = 0.01;
        }
        weather
    })?;

    registry.add_weather({
        let mut weather =
            WeatherDef::new("suspend-particles", WeatherKind::Particle, bundle, store);
        weather.hidden = true;
        {
            let color = Rgba::from_hex("a7c1fa").unwrap_or(Rgba::WHITE);
            let particle = weather.particle_mut();
            particle.color = color;
            particle.noise_color = color;
            particle.particle_region = String::from("particle");
            particle.use_wind_vector = true;
            particle.size_max = 4.0;
            particle.size_min = 1.4;
            particle.min_alpha = 0.5;
            particle.max_alpha = 1.0;
            particle.density = 10000.0;
            particle.base_speed = 0.03;
        }
        weather
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::test_support::test_registry;
    use super::*;

    /// Weather order/names and the `snowing` name quirk (plan 02 §4).
    #[test]
    fn load_order_and_names() {
        let registry = test_registry();
        assert_eq!(registry.weathers().len(), 6, "weather count");
        let names: Vec<&str> = registry
            .weathers()
            .iter()
            .map(|weather| weather.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec![
                "snowing",
                "rain",
                "sandstorm",
                "sporestorm",
                "fog",
                "suspend-particles"
            ]
        );
        let snow = registry.weather(WeatherId::new(0)).unwrap();
        assert_eq!(snow.kind, WeatherKind::Particle);
        assert_eq!(snow.attr(Attribute::Light), -0.15);
        assert_eq!(snow.sound_vol_osc_scl, 1100.0);
        assert_eq!(snow.sound_vol_min, 0.02);

        let sandstorm = registry.weather_by_name("sandstorm").unwrap();
        assert_eq!(sandstorm.duration, 7.0 * TIME_TO_MINUTES);
        assert_eq!(sandstorm.attr(Attribute::Water), -0.1);
        assert_eq!(sandstorm.particle.as_ref().unwrap().density, 1500.0);

        let rain = registry.weather_by_name("rain").unwrap();
        assert_eq!(rain.kind, WeatherKind::Rain);
        assert_eq!(rain.status, registry.status_id("wet").unwrap());
        assert_eq!(rain.rain.as_ref().unwrap().liquid, LiquidId::WATER);

        let suspend = registry.weather_by_name("suspend-particles").unwrap();
        assert!(suspend.hidden);
    }
}
